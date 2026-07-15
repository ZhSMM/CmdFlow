//! 插件 IPC (Phase 7/8/9)
//!
//! 插件目录: `app_data_dir/plugins/`
//! 每个插件一个子目录: `<plugin-name>/{manifest.json, index.js, plugin.wasm}`
//!
//! Phase 7: JS 插件 (QuickJS/Boa 引擎)
//! Phase 8: WASM 插件 (wasmtime)
//!
//! manifest.json 格式:
//! ```json
//! {
//!   "id": "hello-world",
//!   "name": "Hello World",
//!   "version": "1.0.0",
//!   "author": "...",
//!   "description": "...",
//!   "format": "js",  // "js" | "wasm"
//!   "entry": "index.js",  // 或 "plugin.wasm"
//!   "permissions": ["fs.read", "http"]
//! }
//! ```

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::models::Plugin;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub format: String,
    pub entry: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Serialize)]
pub struct PluginInfo {
    pub plugin: Plugin,
    pub has_manifest: bool,
    pub size_bytes: u64,
}

fn plugins_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::other(format!("获取 app_data_dir 失败: {e}")))?;
    Ok(dir.join("plugins"))
}

#[tauri::command]
pub async fn list_plugins(app: AppHandle) -> AppResult<Vec<PluginInfo>> {
    let dir = plugins_dir(&app)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let conn = {
        let state = app.state::<AppState>();
        state.db.get()?
    };

    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| AppError::other(format!("读 plugins 目录失败: {e}")))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        let manifest_str = std::fs::read_to_string(&manifest_path)
            .map_err(|e| AppError::other(format!("读 manifest 失败: {e}")))?;
        let manifest: PluginManifest = match serde_json::from_str(&manifest_str) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("manifest 解析失败 {}: {e}", manifest_path.display());
                continue;
            }
        };

        // 入库 (INSERT OR REPLACE)
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO plugins (id, name, version, author, description, format, entry, manifest, enabled, installed_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?9)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, version=excluded.version, author=excluded.author,
                description=excluded.description, format=excluded.format,
                entry=excluded.entry, manifest=excluded.manifest, updated_at=excluded.updated_at",
            rusqlite::params![
                manifest.id, manifest.name, manifest.version, manifest.author,
                manifest.description, manifest.format, manifest.entry, manifest_str, now,
            ],
        )?;

        // 计算大小
        let entry_path = path.join(&manifest.entry);
        let size = std::fs::metadata(&entry_path).map(|m| m.len()).unwrap_or(0);

        let plugin: Plugin = conn.query_row(
            "SELECT id, name, version, author, description, format, entry, manifest, enabled, installed_at, updated_at
             FROM plugins WHERE id = ?1",
            [&manifest.id],
            |r| Ok(Plugin {
                id: r.get(0)?, name: r.get(1)?, version: r.get(2)?,
                author: r.get(3)?, description: r.get(4)?, format: r.get(5)?,
                entry: r.get(6)?, manifest: r.get(7)?, enabled: r.get(8)?,
                installed_at: r.get(9)?, updated_at: r.get(10)?,
            }),
        )?;

        out.push(PluginInfo {
            plugin,
            has_manifest: true,
            size_bytes: size,
        });
    }

    Ok(out)
}

#[tauri::command]
pub async fn uninstall_plugin(app: AppHandle, id: String) -> AppResult<()> {
    let dir = plugins_dir(&app)?;
    let target = dir.join(&id);
    if target.exists() {
        std::fs::remove_dir_all(&target)
            .map_err(|e| AppError::other(format!("删除插件目录失败: {e}")))?;
    }
    let state = app.state::<AppState>();
    let conn = state.db.get()?;
    conn.execute("DELETE FROM plugins WHERE id = ?1", [&id])?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_plugin(app: AppHandle, id: String, enabled: bool) -> AppResult<()> {
    let state = app.state::<AppState>();
    let conn = state.db.get()?;
    conn.execute(
        "UPDATE plugins SET enabled = ?1 WHERE id = ?2",
        rusqlite::params![enabled as i32, id],
    )?;
    Ok(())
}

/// 注册全局快捷键 Ctrl+Alt+P 唤起启动器独立窗口 (Phase 9.4)
///
/// 原来用 Cmd+Shift+Space 跟 Windows Search / 某些中文输入法冲突，
/// 这台机器上注册时直接报 "HotKey already registered"（被 OS 占用），
/// 启动器改用 Ctrl+Alt+P 通用、且不冲突。
/// 后续要做成可在 settings 里改。
pub fn register_palette_shortcut(app: &AppHandle) -> AppResult<()> {
    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP);
    let gs = app.global_shortcut();

    // 之前 Force Quit / dev 热重启会让 plugin 进程里残留同组合键的 handler，
    // 再 on_shortcut 就会报 "HotKey already registered"。
    // unregister_all 把整个 plugin 管的快捷键都清掉，再装新的 — 反正我们只
    // 用这一组 Ctrl+Alt+P。
    if let Err(e) = gs.unregister_all() {
        tracing::debug!("[palette] unregister_all (正常情况会报 NotRegistered): {e}");
    }

    let app_clone = app.clone();
    gs.on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                // 直接调命令创建/聚焦独立窗口
                let app_handle = app_clone.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = crate::commands::window::show_palette_window(app_handle).await {
                        tracing::warn!("唤起启动器窗口失败: {e}");
                    }
                });
            }
        })
        .map_err(|e| AppError::other(format!("注册全局快捷键失败: {e}")))?;
    tracing::info!("[palette] 全局快捷键 Ctrl+Alt+P 已注册 (启动器)");
    Ok(())
}

/// 调用 JS 插件 (Phase 8 真用 Boa engine 跑)
#[tauri::command]
pub async fn execute_js_plugin(
    app: AppHandle,
    _state: State<'_, AppState>,
    plugin_id: String,
    function: String,
    args: serde_json::Value,
) -> AppResult<serde_json::Value> {
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::timeout;

    // 读插件入口
    let dir = plugins_dir(&app)?;
    let plugin_dir = dir.join(&plugin_id);
    let manifest_path = plugin_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(AppError::not_found(format!("plugin: {plugin_id}")));
    }
    let manifest_str = std::fs::read_to_string(&manifest_path)
        .map_err(|e| AppError::other(format!("读 manifest 失败: {e}")))?;
    let manifest: PluginManifest = serde_json::from_str(&manifest_str)
        .map_err(|e| AppError::other(format!("manifest 解析失败: {e}")))?;
    if manifest.format != "js" {
        return Err(AppError::other(format!(
            "不支持的插件格式: {}",
            manifest.format
        )));
    }

    let entry_path = plugin_dir.join(&manifest.entry);
    let source = std::fs::read_to_string(&entry_path)
        .map_err(|e| AppError::other(format!("读插件入口失败: {e}")))?;

    // 转 args: serde_json::Value -> HashMap
    let params: HashMap<String, serde_json::Value> = match args {
        serde_json::Value::Object(m) => m.into_iter().collect(),
        _ => HashMap::new(),
    };

    // 5s 超时
    let source_clone = source.clone();
    let params_clone = params.clone();
    let exec = tokio::task::spawn_blocking(move || {
        crate::js_runtime::execute(&source_clone, params_clone)
    });

    let res = match timeout(Duration::from_secs(5), exec).await {
        Ok(Ok(r)) => r?,
        Ok(Err(join_err)) => {
            return Err(AppError::other(format!("插件执行 join 失败: {join_err}")));
        }
        Err(_) => {
            return Err(AppError::other(format!("插件 {plugin_id} 执行超时 (5s)")));
        }
    };

    tracing::info!(
        "插件 {plugin_id}::{function} 执行完毕, {} 条日志",
        res.logs.len()
    );
    for log in &res.logs {
        tracing::debug!("  [plugin log] {log}");
    }

    // 把日志包进 result (兼容老接口)
    Ok(serde_json::json!({
        "plugin": plugin_id,
        "function": function,
        "result": res.result,
        "logs": res.logs,
    }))
}

/// 调用 WASM 插件 (Phase 9.3,基于 wasmtime)
///
/// 合约见 `src/wasm_runtime.rs`。
#[tauri::command]
pub async fn execute_wasm_plugin(
    app: AppHandle,
    _state: State<'_, AppState>,
    plugin_id: String,
    function: String,
    args: serde_json::Value,
) -> AppResult<serde_json::Value> {
    use std::time::Duration;
    use tokio::time::timeout;

    // 读插件 manifest 找 entry
    let dir = plugins_dir(&app)?;
    let plugin_dir = dir.join(&plugin_id);
    let manifest_path = plugin_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(AppError::not_found(format!("plugin: {plugin_id}")));
    }
    let manifest_str = std::fs::read_to_string(&manifest_path)
        .map_err(|e| AppError::other(format!("读 manifest 失败: {e}")))?;
    let manifest: PluginManifest = serde_json::from_str(&manifest_str)
        .map_err(|e| AppError::other(format!("manifest 解析失败: {e}")))?;
    if manifest.format != "wasm" {
        return Err(AppError::other(format!(
            "插件 {plugin_id} 不是 wasm 格式 (实际: {})",
            manifest.format
        )));
    }

    let entry_path = plugin_dir.join(&manifest.entry);

    // 转 args: 允许 Object/Array/任何 JSON 值
    let input_value = args;

    // 阻塞执行,加 10s 超时
    let entry_path_clone = entry_path.clone();
    let function_clone = function.clone();
    let exec = tokio::task::spawn_blocking(move || {
        let inst = crate::wasm_runtime::WasmInstance::from_file(&entry_path_clone)?;
        inst.call(&function_clone, &input_value)
    });

    let result = match timeout(Duration::from_secs(10), exec).await {
        Ok(Ok(r)) => r?,
        Ok(Err(join_err)) => {
            return Err(AppError::other(format!("wasm 执行 join 失败: {join_err}")));
        }
        Err(_) => {
            return Err(AppError::other(format!(
                "wasm 插件 {plugin_id} 执行超时 (10s)"
            )));
        }
    };

    tracing::info!("wasm 插件 {plugin_id}::{function} 执行完毕");
    Ok(serde_json::json!({
        "plugin": plugin_id,
        "function": function,
        "format": "wasm",
        "result": result,
    }))
}
