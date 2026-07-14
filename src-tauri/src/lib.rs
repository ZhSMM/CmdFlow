//! CmdFlow - 本地命令编排工具
//!
//! 模块结构：
//! - `commands`: Tauri command handlers (IPC 入口)
//! - `core`:    业务核心 (DAG、执行引擎、调度)
//! - `nodes`:   节点实现
//! - `storage`: SQLite 持久化
//! - `security`: 黑名单、加密

pub mod commands;
pub mod core;
pub mod error;
pub mod js_runtime;
pub mod nodes;
pub mod security;
pub mod state;
pub mod storage;

use tauri::Manager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// 启动 Tauri 应用
pub fn run() {
    // 初始化日志
    init_tracing();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // 初始化应用状态（DB 等）
            let app_handle = app.handle().clone();
            let state = state::AppState::init(&app_handle)?;
            let db = state.db.clone();
            app.manage(state);

            // 启动调度器
            crate::core::scheduler::start(app_handle.clone(), db);

            // 注册全局快捷键 (Cmd+Shift+Space) 唤起启动器
            if let Err(e) = crate::commands::plugin::register_palette_shortcut(&app_handle) {
                tracing::warn!("注册启动器快捷键失败: {e}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // ===== 应用信息 =====
            commands::system::get_app_info,
            // ===== 命令库 =====
            commands::library::list_commands,
            commands::library::get_command,
            commands::library::create_command,
            commands::library::update_command,
            commands::library::delete_command,
            // ===== 工作流 =====
            commands::workflow::list_workflows,
            commands::workflow::get_workflow,
            commands::workflow::create_workflow,
            commands::workflow::update_workflow,
            commands::workflow::delete_workflow,
            commands::workflow::validate_dag,
            commands::workflow::run_workflow,
            commands::workflow::cancel_workflow_execution,
            commands::workflow::list_node_types,
            commands::workflow::get_node_schema,
            // ===== 执行 =====
            commands::execution::preview_command,
            commands::execution::run_command,
            commands::execution::cancel_execution,
            commands::execution::list_executions,
            commands::execution::get_execution,
            // ===== 调度 =====
            commands::schedule::list_schedules,
            commands::schedule::create_schedule,
            commands::schedule::update_schedule,
            commands::schedule::delete_schedule,
            commands::schedule::trigger_schedule_now,
            commands::schedule::validate_cron_expression,
            commands::schedule::describe_cron_expression,
            // ===== 历史 =====
            commands::history::list_history,
            commands::history::get_history_detail,
            commands::history::replay_execution,
            commands::history::get_history_stats,
            commands::history::delete_history,
            commands::history::clear_history,
            commands::history::search_history,
            // ===== 分类树 (Phase 7) =====
            commands::category::list_category_tree,
            commands::category::create_category,
            commands::category::rename_category,
            commands::category::move_category,
            commands::category::delete_category,
            commands::category::move_command,
            commands::category::reorder_categories,
            // ===== 收藏 (Phase 7) =====
            commands::favorite::list_favorites,
            commands::favorite::add_favorite,
            commands::favorite::remove_favorite,
            commands::favorite::reorder_favorites,
            // ===== 插件 (Phase 7) =====
            commands::plugin::list_plugins,
            commands::plugin::uninstall_plugin,
            commands::plugin::toggle_plugin,
            commands::plugin::execute_js_plugin,
            // ===== YAML 导入导出 (Phase 8) =====
            commands::yaml_io::export_workflow,
            commands::yaml_io::import_workflow,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,cmdflow_lib=debug,tauri=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .init();
}
