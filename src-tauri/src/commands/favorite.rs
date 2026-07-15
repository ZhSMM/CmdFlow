//! 收藏 IPC (Phase 7)

use serde::Deserialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::models::{Command, CommandType, Favorite};

#[tauri::command]
pub async fn list_favorites(state: State<'_, AppState>) -> AppResult<Vec<Favorite>> {
    let conn = state.db.get()?;
    let mut stmt = conn.prepare(
        "SELECT f.command_id, f.sort_order, f.created_at,
                c.id, c.name, c.description, c.category_id, c.type, c.current_ver, c.tags, c.created_at, c.updated_at
         FROM favorites f LEFT JOIN commands c ON c.id = f.command_id
         ORDER BY f.sort_order, f.created_at",
    )?;
    let rows = stmt.query_map([], |r| {
        let cmd_id: String = r.get(0)?;
        let cmd: Option<Command> = {
            let tags_json: String = r.get(9)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let cmd_id_c: String = r.get(4)?;
            Some(Command {
                id: cmd_id_c,
                name: r.get(5)?,
                description: r.get(6)?,
                category: r.get(7)?,
                command_type: CommandType::parse(&r.get::<_, String>(8)?)
                    .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
                current_ver: r.get(9)?,
                tags,
                created_at: r.get(10)?,
                updated_at: r.get(11)?,
            })
        };
        Ok(Favorite {
            command_id: cmd_id,
            sort_order: r.get(1)?,
            created_at: r.get(2)?,
            command: cmd,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[tauri::command]
pub async fn add_favorite(state: State<'_, AppState>, command_id: String) -> AppResult<()> {
    let conn = state.db.get()?;
    // 检查命令存在
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM commands WHERE id = ?1)",
            [&command_id],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !exists {
        return Err(AppError::not_found(format!("command: {command_id}")));
    }
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT OR REPLACE INTO favorites (command_id, sort_order, created_at) VALUES (?1, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM favorites), ?2)",
        rusqlite::params![command_id, now],
    )?;
    Ok(())
}

#[tauri::command]
pub async fn remove_favorite(state: State<'_, AppState>, command_id: String) -> AppResult<()> {
    let conn = state.db.get()?;
    conn.execute("DELETE FROM favorites WHERE command_id = ?1", [&command_id])?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ReorderFavoritesInput {
    pub command_ids: Vec<String>,
}

#[tauri::command]
pub async fn reorder_favorites(
    state: State<'_, AppState>,
    input: ReorderFavoritesInput,
) -> AppResult<()> {
    let conn = state.db.get()?;
    let tx = conn.unchecked_transaction()?;
    for (i, cid) in input.command_ids.iter().enumerate() {
        tx.execute(
            "UPDATE favorites SET sort_order = ?1 WHERE command_id = ?2",
            rusqlite::params![i as i32, cid],
        )?;
    }
    tx.commit()?;
    Ok(())
}
