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
        let size = std::fs::metadata(&entry_path)
            .map(|m| m.len())
            .unwrap_or(0);

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

/// 注册全局快捷键 Cmd+Shift+Space 唤起启动器
pub fn register_palette_shortcut(app: &AppHandle) -> AppResult<()> {
    use tauri::Emitter;
    let shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);
    let app_clone = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                // 推一个事件给前端,前端决定是否弹启动器
                let _ = app_clone.emit("palette-toggle", ());
            }
        })
        .map_err(|e| AppError::other(format!("注册全局快捷键失败: {e}")))?;
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
        return Err(AppError::other(format!("不支持的插件格式: {}", manifest.format)));
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
            return Err(AppError::other(format!(
                "插件 {plugin_id} 执行超时 (5s)"
            )));
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
