//! SQLite 连接池
//!
//! 使用 r2d2 + rusqlite。bundled 特性确保不依赖系统 sqlite 库。

use std::path::Path;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OpenFlags;

use crate::error::{AppError, AppResult};
use crate::storage::migrations;

/// 连接池类型别名
pub type DbPool = Pool<SqliteConnectionManager>;
pub type DbConn = r2d2::PooledConnection<SqliteConnectionManager>;

/// 打开一个临时文件数据库 (用于测试)
/// 用 tempfile 给每个测试一个独立的 db 文件,避免 r2d2 内存 DB 共享问题
pub fn open_pool_memory() -> AppResult<DbPool> {
    use std::env;
    let mut tmp = env::temp_dir();
    tmp.push(format!("cmdflow_test_{}.db", uuid::Uuid::new_v4()));
    let _ = std::fs::remove_file(&tmp);
    open_pool(&tmp)
}

/// 打开一个 SQLite 连接池
pub fn open_pool(path: &Path) -> AppResult<DbPool> {
    let manager = SqliteConnectionManager::file(path)
        .with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)
        .with_init(|c| {
            // 每个连接初始化
            c.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA busy_timeout = 5000;",
            )
        });

    let pool = Pool::builder()
        .max_size(8)
        .build(manager)
        .map_err(|e| AppError::other(format!("创建连接池失败: {e}")))?;

    Ok(pool)
}

/// 运行所有迁移
pub fn run_migrations(pool: &DbPool) -> AppResult<()> {
    let mut conn = pool.get()?;
    migrations::run(&mut conn)
}
