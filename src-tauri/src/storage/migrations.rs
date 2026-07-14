//! 数据库迁移
//!
//! 迁移文件位于 `migrations/` 目录，按文件名顺序执行。
//! 使用简单的 `_migrations` 表记录已执行的版本。

use rusqlite::Connection;

use crate::error::AppResult;

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_init",
        include_str!("../../../migrations/0001_init.sql"),
    ),
];

pub fn run(conn: &mut Connection) -> AppResult<()> {
    // 1. 建迁移记录表
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            name TEXT PRIMARY KEY,
            applied_at INTEGER NOT NULL
        );",
    )?;

    // 2. 逐个执行未跑过的迁移
    for (name, sql) in MIGRATIONS {
        let already_applied: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?1)",
                [name],
                |r| r.get(0),
            )?;

        if already_applied {
            tracing::debug!("迁移 {} 已应用，跳过", name);
            continue;
        }

        tracing::info!("应用迁移: {}", name);

        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO _migrations (name, applied_at) VALUES (?1, ?2)",
            rusqlite::params![name, chrono::Utc::now().timestamp()],
        )?;
        tx.commit()?;

        tracing::info!("迁移 {} 完成", name);
    }

    Ok(())
}
