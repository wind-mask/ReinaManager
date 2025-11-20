/// 一些特化的扫描逻辑，有待增加灵活性
use std::{
    collections::VecDeque,
    fs::read_dir,
    path::{self, Path},
};

use log::debug;
use tauri::command;
#[derive(Debug)]
enum DirKind {
    Lib,
    Game(GameKind),
    Unknown,
}
#[derive(Debug)]
enum GameKind {
    Compressed,
    Dir,
}
fn scan_dir_kind(dir_path: &str) -> DirKind {
    assert!(Path::new(dir_path).is_dir());
    let entries = read_dir(Path::new(dir_path)).unwrap();
    let mut exe = false;
    let mut xp3 = false;
    let mut sig = false;
    let mut arc = false;
    let mut _dll = false;
    let mut dat = false;
    let mut pfs = false;
    let mut gar = false;
    let mut iar = false;
    let mut pak = false;
    let mut has_files = false;
    let mut has_dirs = false;
    let mut only_compressd_files = true;
    for entry in entries {
        let entry = entry.unwrap();
        if !entry.path().is_file() {
            if entry.path().is_dir() {
                has_dirs = true;
            }
            continue;
        }
        has_files = true;
        let file_name = entry.file_name();
        let file_name = file_name.to_str().unwrap();
        let first_dot = file_name.find('.');
        if let Some(pos) = first_dot {
            let ext = &file_name[pos..].to_lowercase();
            if !(ext.ends_with(".zip")) && !(ext.ends_with(".7z")) && !(ext.ends_with(".rar")) {
                only_compressd_files = false;
            }
            match ext.as_str() {
                ".exe" => {
                    exe = true;
                }
                ".dll" => {
                    _dll = true;
                }
                ".dat" => {
                    dat = true;
                }
                ".pfs" => {
                    pfs = true;
                }
                ".pak" => {
                    pak = true;
                }
                ".arc" => {
                    arc = true;
                }
                ".gar" => {
                    gar = true;
                }
                ".iar" => {
                    iar = true;
                }
                ".xp3" => {
                    xp3 = true;
                }
                ".exe.sig" | ".xp3.sig" => {
                    sig = true;
                }
                _ => {
                    continue;
                }
            }
        }
    }
    if has_dirs && !has_files {
        return DirKind::Lib;
    }
    if has_dirs && has_files && only_compressd_files {
        return DirKind::Lib;
    }
    if exe && (xp3 || sig || arc || pfs || pak || dat || gar || iar) {
        return DirKind::Game(GameKind::Dir);
    }
    let mut compressed = 0;
    let mut volume_count = 0;
    let mut sub_game_dir = 0;
    for entry in read_dir(Path::new(dir_path)).unwrap() {
        let entry = entry.unwrap();
        // 如果只有一个子文件夹判定为游戏目录,或者有特殊打包格式文件如iso，或只有一个压缩文件
        if entry.path().is_dir() {
            let sub_dir_kind = scan_dir_kind(entry.path().to_str().unwrap());
            if let DirKind::Game(_) = sub_dir_kind {
                sub_game_dir += 1;
            }
            // 如果有 savedata 之类的文件夹，也算游戏目录
            let dir_name = entry.file_name();
            let dir_name = dir_name.to_str().unwrap().to_lowercase();
            if dir_name == "savedata" {
                return DirKind::Game(GameKind::Dir);
            }
        }
        if entry.path().is_file() {
            let file_name = entry.file_name();
            let file_name = file_name.to_str().unwrap().to_lowercase();

            if file_name.ends_with(".zip")
                || file_name.ends_with(".7z")
                || file_name.ends_with(".rar")
                || file_name.ends_with(".iso")
            {
                compressed += 1;
            }
            // 考虑分卷压缩
            if file_name.contains(".part")
                && (file_name.ends_with(".rar") || file_name.ends_with(".7z"))
            {
                volume_count += 1;
            }
        }
    }
    if sub_game_dir == 1 {
        return DirKind::Game(GameKind::Dir);
    }
    if compressed == 1 {
        return DirKind::Game(GameKind::Compressed);
    }
    if volume_count == compressed && compressed > 0 {
        return DirKind::Game(GameKind::Compressed);
    }

    DirKind::Unknown
}
fn scantogaldirs(lib_path: &str) -> Vec<String> {
    let mut game_dirs = Vec::new();
    let mut dirs_to_process = VecDeque::new();
    dirs_to_process.push_back(lib_path.to_string());

    while let Some(current_path) = dirs_to_process.pop_front() {
        let entries: Vec<_> = match read_dir(Path::new(&current_path)) {
            Ok(entries) => entries.filter_map(Result::ok).collect(),
            Err(_) => continue,
        };

        for d in entries {
            if d.path().is_dir() {
                let dir_kind = scan_dir_kind(d.path().to_str().unwrap());
                match dir_kind {
                    DirKind::Lib => {
                        // 添加到待处理队列，而不是递归调用
                        dirs_to_process.push_back(d.path().to_str().unwrap().to_string());
                    }
                    DirKind::Game(GameKind::Dir) => {
                        debug!("Found game directory: {}", d.path().to_str().unwrap());
                        game_dirs.push(d.path().to_str().unwrap().to_string());
                    }
                    DirKind::Game(GameKind::Compressed) => {
                        debug!(
                            "Found compressed game directory: {}",
                            d.path().to_str().unwrap()
                        );
                    }
                    DirKind::Unknown => {}
                }
            }
        }
    }

    game_dirs
}
#[command]
pub fn scan_game_library(path: String) -> Result<Vec<String>, String> {
    let scan_path = path::PathBuf::from(path);
    if !scan_path.exists() || !scan_path.is_dir() {
        Err("Provided path does not exist or is not a directory".to_string())
    } else if let Some(path) = scan_path.to_str() {
        let v = scantogaldirs(path);
        Ok(v)
    } else {
        Err("Invalid path string".to_string())
    }
}
