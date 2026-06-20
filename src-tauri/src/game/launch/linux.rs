use crate::database::repository::games_repository::GamesRepository;
use crate::game::monitor::{get_connection, get_manager_proxy, monitor_game, stop_game_session};
use log::{debug, info};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime, State, command};
use tauri_plugin_store::StoreExt;

#[derive(Debug, Serialize, Deserialize)]
pub struct LaunchResult {
    success: bool,
    message: String,
    process_id: Option<u32>,
    systemd_unit: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StopResult {
    success: bool,
    message: String,
    terminated_count: u32,
}

#[command]
pub async fn launch_game<R: Runtime>(
    app_handle: AppHandle<R>,
    db: State<'_, DatabaseConnection>,
    game_id: u32,
    args: Option<Vec<String>>,
) -> Result<LaunchResult, String> {
    let game = GamesRepository::find_by_id(db.inner(), game_id as i32)
        .await
        .map_err(|e| format!("查询游戏失败: {}", e))?
        .ok_or_else(|| format!("游戏不存在: {}", game_id))?;
    let game_path = game.localpath.ok_or_else(|| "游戏路径未设置".to_string())?;

    if !Path::new(&game_path).exists() {
        return Err(format!("游戏可执行文件不存在: {}", game_path));
    }

    let game_dir = match Path::new(&game_path).parent() {
        Some(dir) => dir,
        None => return Err("无法获取游戏目录路径".to_string()),
    };

    let exe_name = match Path::new(&game_path).file_name() {
        Some(name) => name,
        None => return Err("无法获取游戏可执行文件名".to_string()),
    };

    let systemd_unit_name = format!("reina_game_{}.service", game_id);
    let _ = check_unit_or_reset_failed(&systemd_unit_name).await;

    let linux_launch_command = {
        let cmd = app_handle
            .store("settings.json")
            .ok()
            .and_then(|store| store.get("linux_launch_command"))
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "wine".to_string());
        expand_path(&app_handle, &cmd)
    };

    // 确定执行命令和参数
    // 对于 .exe 文件: 用 linux_launch_command 作为命令，游戏文件作为参数
    // 对于其他文件: 直接用文件本身作为命令
    let (exec_path, exec_args) = if exe_name.to_string_lossy().ends_with(".exe") {
        // .exe 文件: wine game.exe [user_args...]
        let mut cmd_args = vec![linux_launch_command.clone(), game_path.clone()];
        if let Some(ref arguments) = args {
            cmd_args.extend(arguments.iter().cloned());
        }
        (linux_launch_command.clone(), cmd_args)
    } else {
        // 其他文件: ./game [user_args...]
        let mut cmd_args = vec![game_path.clone()];
        if let Some(ref arguments) = args {
            cmd_args.extend(arguments.iter().cloned());
        }
        (game_path.clone(), cmd_args)
    };
    // 从当前进程导入环境变量
    let env_vars: Vec<String> = std::env::vars()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();

    debug!(
        "准备启动游戏 game_id={} unit={} exec_path={:?} exec_args={:?} cwd={:?} env_count={}",
        game_id,
        systemd_unit_name,
        exec_path,
        exec_args,
        game_dir,
        env_vars.len()
    );

    // 使用 D-Bus StartTransientUnit 创建 service
    let manager = get_manager_proxy()
        .await
        .map_err(|e| format!("连接到 systemd 失败，无法启动游戏 {}: {}", game_id, e))?;

    use zbus::zvariant::{OwnedValue, Value};

    // 构建 properties
    // Type=exec: 直接执行程序
    let type_prop = (
        "Type".to_string(),
        OwnedValue::try_from(Value::from("exec".to_string()))
            .map_err(|e| format!("构建 Type 属性失败: {}", e))?,
    );

    // ExecStart 短格式 a(sasb): (path, args_with_argv0, ignore_error)
    let exec_start_entry: (String, Vec<String>, bool) = (
        exec_path, // 可执行文件路径
        exec_args, // 参数列表（已包含 argv[0]）
        false,     // 不忽略错误
    );
    let exec_start_prop = (
        "ExecStart".to_string(),
        OwnedValue::try_from(Value::from(vec![exec_start_entry]))
            .map_err(|e| format!("构建 ExecStart 属性失败: {}", e))?,
    );

    // WorkingDirectory
    let working_dir_prop = (
        "WorkingDirectory".to_string(),
        OwnedValue::try_from(Value::from(game_dir.to_string_lossy().to_string()))
            .map_err(|e| format!("构建 WorkingDirectory 属性失败: {}", e))?,
    );

    // Environment=["KEY1=value1", ...]
    let env_prop = (
        "Environment".to_string(),
        OwnedValue::try_from(Value::from(env_vars))
            .map_err(|e| format!("构建 Environment 属性失败: {}", e))?,
    );

    // Delegate=yes (用于资源控制)
    let delegate_prop = (
        "Delegate".to_string(),
        OwnedValue::try_from(Value::from(true))
            .map_err(|e| format!("构建 Delegate 属性失败: {}", e))?,
    );

    let properties = vec![
        type_prop,
        exec_start_prop,
        working_dir_prop,
        env_prop,
        delegate_prop,
    ];
    // aux 参数（空）
    let aux: Vec<(String, Vec<(String, OwnedValue)>)> = Vec::new();

    match manager
        .start_transient_unit(
            systemd_unit_name.clone(),
            "replace".to_string(),
            properties,
            aux,
        )
        .await
    {
        Ok(job_path) => {
            info!(
                "游戏启动成功 game_id={} unit={} job={:?}",
                game_id, systemd_unit_name, job_path
            );

            // 等待一小段时间让进程启动
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            // 获取 service 的主进程 PID
            let process_id = get_service_main_pid(&systemd_unit_name).await.unwrap_or(0);

            info!("获取到游戏进程 PID={}", process_id);

            monitor_game(
                app_handle.clone(),
                game_id,
                process_id,
                systemd_unit_name.clone(),
            )
            .await;

            Ok(LaunchResult {
                success: true,
                message: format!(
                    "成功启动游戏: {}，工作目录: {:?}",
                    exe_name.to_string_lossy(),
                    game_dir
                ),
                process_id: Some(process_id),
                systemd_unit: Some(systemd_unit_name),
            })
        }
        Err(e) => Err(format!("启动游戏失败: {}，目录: {:?}", e, game_dir)),
    }
}

/// 获取 systemd service 的主进程 PID
async fn get_service_main_pid(unit_name: &str) -> Result<u32, String> {
    let manager = get_manager_proxy()
        .await
        .map_err(|e| format!("连接到 systemd 失败，无法获取进程 PID: {}", e))?;

    let unit_path = manager
        .get_unit(unit_name.to_string())
        .await
        .map_err(|e| format!("获取单元 {} 失败: {}", unit_name, e))?;

    let conn = get_connection()
        .await
        .map_err(|e| format!("连接到 systemd 失败: {}", e))?;

    // 使用 ServiceProxy 来获取 MainPID
    let service_proxy = zbus_systemd::systemd1::ServiceProxy::new(conn, unit_path)
        .await
        .map_err(|e| format!("创建 Service 代理失败: {}", e))?;

    // 获取主进程 PID
    let main_pid = service_proxy
        .main_pid()
        .await
        .map_err(|e| format!("获取 MainPID 失败: {}", e))?;

    Ok(main_pid)
}

#[command]
pub async fn stop_game(game_id: u32) -> Result<StopResult, String> {
    match stop_game_session(game_id).await {
        Ok(terminated_count) => Ok(StopResult {
            success: true,
            message: format!("成功停止游戏 {}，终止进程数: {}", game_id, terminated_count),
            terminated_count,
        }),
        Err(e) => Err(format!("停止游戏 {} 失败: {}", game_id, e)),
    }
}

fn expand_path<R: Runtime>(app_handle: &AppHandle<R>, path: &str) -> String {
    if path.starts_with('~') {
        if let Ok(home_dir) = app_handle.path().home_dir() {
            path.replacen('~', &home_dir.to_string_lossy(), 1)
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    }
}

/// 检查 systemd unit 的状态，如果是 failed 则重置它
/// 返回 bool 值表示 unit 是否已经存在
/// # Arguments
/// * `systemd_unit_name` - systemd 单元名称
///
/// # Returns
/// bool - 如果 unit 已存在则返回 true，否则返回 false
async fn check_unit_or_reset_failed(systemd_unit_name: &str) -> Result<bool, String> {
    let proxy = get_manager_proxy().await.map_err(|e| {
        format!(
            "连接到 systemd 失败，无法检查或重置单元 {}: {}",
            systemd_unit_name, e
        )
    })?;
    match proxy.get_unit(systemd_unit_name.to_string()).await {
        Ok(u) => {
            let conn = get_connection().await.map_err(|e| {
                format!(
                    "连接到 systemd 失败，无法检查或重置单元 {}: {}",
                    systemd_unit_name, e
                )
            })?;
            match zbus_systemd::systemd1::UnitProxy::new(conn, u).await {
                Ok(unit_proxy) => {
                    let active_state = unit_proxy
                        .active_state()
                        .await
                        .map_err(|e| format!("获取单元 {} 状态失败: {}", systemd_unit_name, e))?;
                    if active_state == "failed" {
                        proxy
                            .reset_failed_unit(systemd_unit_name.to_string())
                            .await
                            .map_err(|e| {
                                format!("重置单元 {} 状态失败: {}", systemd_unit_name, e)
                            })?;
                        info!("单元 {} 已被重置", systemd_unit_name);
                    }
                    Ok(true)
                }
                Err(_) => Ok(false),
            }
        }
        Err(_) => Ok(false),
    }
}
