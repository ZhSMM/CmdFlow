//! 系统级 IPC: 应用信息、健康检查等

use serde::Serialize;
use tauri::State;

use crate::error::AppResult;
use crate::state::AppState;

#[derive(Serialize)]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub started_at: i64,
    pub uptime_seconds: i64,
}

#[tauri::command]
pub fn get_app_info(state: State<'_, AppState>) -> AppResult<AppInfo> {
    let now = chrono::Utc::now();
    let uptime = (now - state.started_at).num_seconds();

    Ok(AppInfo {
        name: "CmdFlow",
        version: env!("CARGO_PKG_VERSION"),
        started_at: state.started_at.timestamp(),
        uptime_seconds: uptime,
    })
}
