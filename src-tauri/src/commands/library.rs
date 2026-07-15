//! 命令库 IPC

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::storage::models::{Command, CommandDetail, CommandType, CommandVersion, Param};

#[tauri::command]
pub fn list_commands(
    state: State<'_, AppState>,
    category: Option<String>,
    search: Option<String>,
) -> AppResult<Vec<Command>> {
    let conn = state.db.get()?;
    let mut sql = String::from(
        "SELECT id, name, description, category, type, current_ver, tags, created_at, updated_at
         FROM commands WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(c) = category {
        sql.push_str(" AND category = ?");
        params.push(Box::new(c));
    }
    if let Some(s) = search {
        sql.push_str(" AND (name LIKE ? OR description LIKE ?)");
        let pat = format!("%{s}%");
        params.push(Box::new(pat.clone()));
        params.push(Box::new(pat));
    }
    sql.push_str(" ORDER BY updated_at DESC");

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(&*param_refs, |r| {
        let tags_json: String = r.get(6)?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        Ok(Command {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            category: r.get(3)?,
            command_type: CommandType::parse(&r.get::<_, String>(4)?)
                .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
            current_ver: r.get(5)?,
            tags,
            created_at: r.get(7)?,
            updated_at: r.get(8)?,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[tauri::command]
pub fn get_command(state: State<'_, AppState>, id: String) -> AppResult<CommandDetail> {
    let conn = state.db.get()?;

    let cmd: Command = conn.query_row(
        "SELECT id, name, description, category, type, current_ver, tags, created_at, updated_at
         FROM commands WHERE id = ?1",
        [&id],
        |r| {
            let tags_json: String = r.get(6)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok(Command {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                category: r.get(3)?,
                command_type: CommandType::parse(&r.get::<_, String>(4)?)
                    .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
                current_ver: r.get(5)?,
                tags,
                created_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        },
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found(format!("command:{id}")),
        other => AppError::Database(other),
    })?;

    let version: CommandVersion = conn.query_row(
        "SELECT id, command_id, version, template, working_dir, env, timeout_ms, shell, note, created_at
         FROM command_versions WHERE command_id = ?1 AND version = ?2",
        rusqlite::params![&id, cmd.current_ver],
        |r| Ok(CommandVersion {
            id: r.get(0)?,
            command_id: r.get(1)?,
            version: r.get(2)?,
            template: r.get(3)?,
            working_dir: r.get(4)?,
            env: r.get::<_, Option<String>>(5)?.and_then(|s| serde_json::from_str(&s).ok()),
            timeout_ms: r.get(6)?,
            shell: r.get(7)?,
            note: r.get(8)?,
            created_at: r.get(9)?,
        }),
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, command_ver_id, name, label, type, required, default_value, options, validation, sensitive, description, sort_order
         FROM params WHERE command_ver_id = ?1 ORDER BY sort_order, name",
    )?;
    let params_iter = stmt.query_map([version.id], |r| {
        Ok(Param {
            id: r.get(0)?,
            command_ver_id: r.get(1)?,
            name: r.get(2)?,
            label: r.get(3)?,
            param_type: r.get(4)?,
            required: r.get(5)?,
            default_value: r
                .get::<_, Option<String>>(6)?
                .and_then(|s| serde_json::from_str(&s).ok()),
            options: r
                .get::<_, Option<String>>(7)?
                .and_then(|s| serde_json::from_str(&s).ok()),
            validation: r
                .get::<_, Option<String>>(8)?
                .and_then(|s| serde_json::from_str(&s).ok()),
            sensitive: r.get(9)?,
            description: r.get(10)?,
            sort_order: r.get(11)?,
        })
    })?;
    let mut params_vec = Vec::new();
    for p in params_iter {
        params_vec.push(p?);
    }

    Ok(CommandDetail {
        command: cmd,
        version,
        params: params_vec,
    })
}

#[derive(serde::Deserialize)]
pub struct CreateCommandInput {
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "type")]
    pub command_type: CommandType,
    pub template: String,
    pub working_dir: Option<String>,
    pub env: Option<serde_json::Value>,
    pub timeout_ms: Option<i64>,
    pub shell: Option<String>,
    pub tags: Option<Vec<String>>,
    pub params: Option<Vec<CreateParamInput>>,
}

#[derive(serde::Deserialize)]
pub struct CreateParamInput {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
    pub options: Option<serde_json::Value>,
    pub validation: Option<serde_json::Value>,
    pub sensitive: bool,
    pub description: Option<String>,
    pub sort_order: i32,
}

#[tauri::command]
pub fn create_command(state: State<'_, AppState>, input: CreateCommandInput) -> AppResult<String> {
    let conn = state.db.get()?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let tags_json = serde_json::to_string(&input.tags.unwrap_or_default())?;

    let tx = conn.unchecked_transaction()?;

    tx.execute(
        "INSERT INTO commands (id, name, description, category, type, current_ver, tags, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8)",
        rusqlite::params![
            &id,
            &input.name,
            &input.description,
            &input.category,
            input.command_type.as_str(),
            &tags_json,
            now,
            now,
        ],
    )?;

    let env_str = input.env.as_ref().map(serde_json::to_string).transpose()?;
    tx.execute(
        "INSERT INTO command_versions (command_id, version, template, working_dir, env, timeout_ms, shell, created_at)
         VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            &id,
            &input.template,
            &input.working_dir,
            &env_str,
            &input.timeout_ms,
            &input.shell,
            now,
        ],
    )?;

    let ver_id: i64 = tx.query_row(
        "SELECT id FROM command_versions WHERE command_id = ?1 AND version = 1",
        [&id],
        |r| r.get(0),
    )?;

    if let Some(params) = input.params {
        for p in params {
            tx.execute(
                "INSERT INTO params (id, command_ver_id, name, label, type, required, default_value, options, validation, sensitive, description, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    ver_id,
                    &p.name,
                    &p.label,
                    &p.param_type,
                    p.required as i32,
                    p.default_value.as_ref().map(serde_json::to_string).transpose()?,
                    p.options.as_ref().map(serde_json::to_string).transpose()?,
                    p.validation.as_ref().map(serde_json::to_string).transpose()?,
                    p.sensitive as i32,
                    &p.description,
                    p.sort_order,
                ],
            )?;
        }
    }

    tx.commit()?;
    Ok(id)
}

#[derive(serde::Deserialize)]
pub struct UpdateCommandInput {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub template: Option<String>,
    pub working_dir: Option<String>,
    pub env: Option<serde_json::Value>,
    pub timeout_ms: Option<i64>,
    pub shell: Option<String>,
    pub params: Option<Vec<CreateParamInput>>,
}

#[tauri::command]
pub fn update_command(state: State<'_, AppState>, input: UpdateCommandInput) -> AppResult<()> {
    let conn = state.db.get()?;
    let now = chrono::Utc::now().timestamp();
    let tx = conn.unchecked_transaction()?;

    // commands 表
    let mut updates: Vec<&str> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(n) = &input.name {
        updates.push("name = ?");
        args.push(Box::new(n.clone()));
    }
    if let Some(d) = &input.description {
        updates.push("description = ?");
        args.push(Box::new(d.clone()));
    }
    if let Some(c) = &input.category {
        updates.push("category = ?");
        args.push(Box::new(c.clone()));
    }
    if let Some(t) = &input.tags {
        updates.push("tags = ?");
        args.push(Box::new(serde_json::to_string(t)?));
    }
    if !updates.is_empty() {
        updates.push("updated_at = ?");
        args.push(Box::new(now));
        args.push(Box::new(input.id.clone()));
        let sql = format!("UPDATE commands SET {} WHERE id = ?", updates.join(", "));
        let arg_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        tx.execute(&sql, &*arg_refs)?;
    }

    // command_versions 表
    let mut v_updates: Vec<&str> = Vec::new();
    let mut v_args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(t) = &input.template {
        v_updates.push("template = ?");
        v_args.push(Box::new(t.clone()));
    }
    if let Some(d) = &input.working_dir {
        v_updates.push("working_dir = ?");
        v_args.push(Box::new(d.clone()));
    }
    if let Some(e) = &input.env {
        v_updates.push("env = ?");
        v_args.push(Box::new(serde_json::to_string(e)?));
    }
    if let Some(t) = &input.timeout_ms {
        v_updates.push("timeout_ms = ?");
        v_args.push(Box::new(*t));
    }
    if let Some(s) = &input.shell {
        v_updates.push("shell = ?");
        v_args.push(Box::new(s.clone()));
    }
    if !v_updates.is_empty() {
        v_args.push(Box::new(input.id.clone()));
        let sql = format!(
            "UPDATE command_versions SET {} WHERE command_id = ? AND version = (SELECT current_ver FROM commands WHERE id = ?)",
            v_updates.join(", ")
        );
        let arg_refs: Vec<&dyn rusqlite::ToSql> = v_args.iter().map(|a| a.as_ref()).collect();
        tx.execute(&sql, &*arg_refs)?;
    }

    // params: 简单实现 - 全删全插
    if let Some(params) = input.params {
        let ver_id: i64 = tx.query_row(
            "SELECT id FROM command_versions WHERE command_id = ?1 AND version = (SELECT current_ver FROM commands WHERE id = ?1)",
            [&input.id],
            |r| r.get(0),
        )?;
        tx.execute("DELETE FROM params WHERE command_ver_id = ?1", [ver_id])?;
        for p in params {
            tx.execute(
                "INSERT INTO params (id, command_ver_id, name, label, type, required, default_value, options, validation, sensitive, description, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    uuid::Uuid::new_v4().to_string(),
                    ver_id,
                    &p.name,
                    &p.label,
                    &p.param_type,
                    p.required as i32,
                    p.default_value.as_ref().map(serde_json::to_string).transpose()?,
                    p.options.as_ref().map(serde_json::to_string).transpose()?,
                    p.validation.as_ref().map(serde_json::to_string).transpose()?,
                    p.sensitive as i32,
                    &p.description,
                    p.sort_order,
                ],
            )?;
        }
        // 顺手更新 commands.updated_at
        tx.execute(
            "UPDATE commands SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, &input.id],
        )?;
    }

    tx.commit()?;
    Ok(())
}

#[tauri::command]
pub fn delete_command(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.get()?;
    conn.execute("DELETE FROM commands WHERE id = ?1", [&id])?;
    Ok(())
}
