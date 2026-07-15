//! 应用全局状态

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::core::executor::ExecutionRegistry;
use crate::error::{AppError, AppResult};
use crate::storage::db::{self, DbPool};
use crate::storage::seed;

pub struct AppState {
    pub db: Arc<DbPool>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub execution_registry: Arc<ExecutionRegistry>,
}

impl AppState {
    pub fn init(app: &AppHandle) -> AppResult<Self> {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::other(format!("获取 app_data_dir 失败: {e}")))?;
        std::fs::create_dir_all(&app_data_dir)?;
        let db_path = app_data_dir.join("cmdflow.db");

        tracing::info!("数据库路径: {}", db_path.display());

        let pool = db::open_pool(&db_path)?;
        db::run_migrations(&pool)?;
        let pool_arc = Arc::new(pool);

        // 首次启动注入种子命令 (Phase 10, 修 favorites 找不到表)
        seed::maybe_seed(&pool_arc);

        Ok(Self {
            db: pool_arc,
            started_at: chrono::Utc::now(),
            execution_registry: Arc::new(ExecutionRegistry::new()),
        })
    }
}
