//! 多窗口管理 (Phase 9.4)
//!
//! 主要能力:
//! - 启动器独立窗口 (palette window): 透明、无边框、置顶、固定大小

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::error::{AppError, AppResult};

/// palette 窗口的 label (Tauri 全局唯一)
pub const PALETTE_LABEL: &str = "palette";

/// 显示启动器独立窗口。如果已存在则聚焦,否则新建。
#[tauri::command]
pub async fn show_palette_window(app: AppHandle) -> AppResult<()> {
    // 如果已存在: 取消最小化 + 聚焦
    if let Some(existing) = app.get_webview_window(PALETTE_LABEL) {
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    // 新建: 透明 + 无边框 + 置顶 + 固定大小
    // URL: 同一个前端应用,带 ?window=palette query,前端据此判断独立渲染
    let url = WebviewUrl::App("index.html?window=palette".into());

    WebviewWindowBuilder::new(&app, PALETTE_LABEL, url)
        .title("CmdFlow 启动器")
        .inner_size(720.0, 480.0)
        .min_inner_size(480.0, 320.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .center()
        .focused(true)
        .build()
        .map_err(|e| AppError::other(format!("创建 palette 窗口失败: {e}")))?;

    Ok(())
}

/// 隐藏启动器窗口
#[tauri::command]
pub async fn hide_palette_window(app: AppHandle) -> AppResult<()> {
    if let Some(win) = app.get_webview_window(PALETTE_LABEL) {
        let _ = win.hide();
    }
    Ok(())
}
