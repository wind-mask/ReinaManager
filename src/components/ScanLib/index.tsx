import { fetchBgmByName } from "@/api/bgm";
import { fetchMixedData } from "@/api/mixed";
import { fetchVndbByName } from "@/api/vndb";
import { useModal } from "@/components/Toolbar";
import { useStore } from "@/store/";
import type { FullGameData } from "@/types";
import { handleFolder, trimDirnameToSearchName } from "@/utils";
import AddIcon from "@mui/icons-material/Add";
import FileOpenIcon from "@mui/icons-material/FileOpen";
import {
	Alert,
	Button,
	CircularProgress,
	Dialog,
	DialogActions,
	DialogContent,
	DialogTitle,
	LinearProgress,
	TextField,
} from "@mui/material";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { useState } from "react";

import { useTranslation } from "react-i18next";

const ScanLib: React.FC = () => {
	const { t } = useTranslation();
	const { bgmToken, games, apiSource, addGame } = useStore();
	const { isopen, handleOpen, handleClose } = useModal();
	const [error, setError] = useState("");
	const [loading, setLoading] = useState(false);
	const [path, setPath] = useState("");
	const [currentIndex, setCurrentIndex] = useState(0);
	const [totalGames, setTotalGames] = useState(0);

	// 辅助函数：从路径中提取文件夹名
	const getParentFolderName = (filePath: string): string => {
		const normalized = filePath.replace(/\\/g, "/");
		const parts = normalized.split("/").filter(Boolean);
		return parts[parts.length - 1] || "";
	};
	const handleSubmit = async () => {
		try {
			setLoading(true);
			setError("");
			// 调用 Rust 命令获取游戏路径列表
			let gamePaths = await invoke<string[]>("scan_game_library", { path });
			if (!gamePaths || gamePaths.length === 0) {
				setError(t("components.ScanLib.noGamesFound"));
				setTimeout(() => setError(""), 5000);
				setLoading(false);
				return;
			}
			gamePaths = gamePaths.filter((p) => {
				games.some((game) => game.localpath === p) === false;
			});
			setTotalGames(gamePaths.length);

			// 批量处理游戏 - 每批处理 3 个
			const batchSize = 3;
			for (let i = 0; i < gamePaths.length; i += batchSize) {
				const batch = gamePaths.slice(i, i + batchSize);

				await Promise.allSettled(
					batch.map(async (gamePath) => {
						try {
							const folderName = getParentFolderName(gamePath);
							const searchName = trimDirnameToSearchName(folderName);

							let apiData: FullGameData | null = null;

							// 根据 API 源获取数据
							if (apiSource === "bgm") {
								const result = await fetchBgmByName(searchName, bgmToken);
								if (typeof result !== "string") {
									apiData = result;
								}
							} else if (apiSource === "vndb") {
								const result = await fetchVndbByName(searchName);
								if (typeof result !== "string") {
									apiData = result;
								}
							} else {
								const { bgm_data, vndb_data } = await fetchMixedData({
									name: searchName,
									BGM_TOKEN: bgmToken,
								});

								if (bgm_data || vndb_data) {
									apiData = {
										game: {
											...bgm_data?.game,
											...vndb_data?.game,
											id_type: "mixed",
										},
										bgm_data: bgm_data?.bgm_data || null,
										vndb_data: vndb_data?.vndb_data || null,
										other_data: null,
									};
								}
							}

							// 构建游戏数据
							const gameData: FullGameData = apiData || {
								game: {
									localpath: gamePath,
									autosave: 0,
									clear: 0,
									id_type: "custom",
								},
								bgm_data: null,
								vndb_data: null,
								other_data: {
									name: searchName,
									image: "",
									summary: null,
									tags: null,
									developer: null,
								},
							};

							gameData.game.localpath = gamePath;
							gameData.game.autosave = 0;
							gameData.game.clear = 0;

							// 检查是否已存在
							const exists = games.some((game) => {
								return (
									(gameData.game.bgm_id &&
										game.bgm_id === gameData.game.bgm_id) ||
									(gameData.game.vndb_id &&
										game.vndb_id === gameData.game.vndb_id) ||
									game.localpath === gamePath
								);
							});

							if (!exists) {
								addGame(gameData);
							}
						} catch (err) {
							console.error(`Failed to process ${gamePath}:`, err);
						}
					}),
				);

				// 更新进度
				setCurrentIndex(Math.min(i + batchSize, gamePaths.length));
			}

			setLoading(false);
			handleClose();
		} catch (error) {
			const errorMessage =
				error instanceof Error
					? error.message
					: t("components.ScanLib.unknownError");
			setError(errorMessage);
			setTimeout(() => setError(""), 5000);
			setLoading(false);
		}
	};
	return (
		<>
			{/* 添加游戏按钮，点击后弹窗打开 */}
			<Button onClick={handleOpen} startIcon={<AddIcon />}>
				{t("components.ScanLib.scanLibrary")}
			</Button>
			{/* 添加游戏弹窗 */}
			<Dialog
				open={isopen}
				onClose={(_, reason) => {
					// 加载时防止关闭弹窗
					if (reason !== "backdropClick" && !loading) {
						handleClose();
					}
				}}
				closeAfterTransition={false}
				aria-labelledby="sacnlib-dialog-title"
			>
				{/* 错误提示 */}
				{error && <Alert severity="error">{error}</Alert>}
				<DialogTitle>{t("components.ScanLib.scanLibrary")}</DialogTitle>
				{/* 弹窗内容区域 */}
				<DialogContent>
					{/* 在这里添加扫描游戏库的表单内容 */}
					{/* 选择库文件夹 */}
					<Button
						className="w-md"
						variant="contained"
						onClick={async () => {
							const result = await handleFolder();
							if (result) setPath(result);
						}}
						startIcon={<FileOpenIcon />}
						disabled={!isTauri()}
					>
						{t("components.ScanLib.selectFolder")}
					</Button>
					<p>
						<TextField
							required
							margin="dense"
							id="name"
							name="game-name"
							label={t("components.ScanLib.selectFolder")}
							type="text"
							fullWidth
							variant="standard"
							autoComplete="off"
							value={path}
							onChange={(event) => setPath(event.target.value)}
						/>
					</p>
					{loading && totalGames > 0 && (
						<div style={{ marginTop: 16 }}>
							<p>
								{t("components.ScanLib.processing")}: {currentIndex}/
								{totalGames}
							</p>
							<LinearProgress
								variant="determinate"
								value={(currentIndex / totalGames) * 100}
							/>
						</div>
					)}
				</DialogContent>
				<DialogActions>
					{/* 取消按钮 */}
					<Button
						variant="outlined"
						onClick={() => {
							setPath("");
							setPath("");
							handleClose();
						}}
						disabled={loading}
					>
						{t("components.ScanLib.cancel") || "取消"}
					</Button>
					{/* 确认按钮 */}
					<Button
						variant="contained"
						onClick={handleSubmit}
						disabled={path === "" || loading}
						startIcon={loading ? <CircularProgress size={20} /> : null}
					>
						{loading
							? // ? t("components.AddModal.processing")
								t("components.ScanLib.scanning")
							: t("components.ScanLib.confirm") || "确认"}
					</Button>
				</DialogActions>
			</Dialog>
		</>
	);
};
export default ScanLib;
