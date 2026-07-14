//! 执行 IPC: run_command / cancel_execution / list_executions / get_execution

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::core::executor::{self, ExecutionSpec};
use crate::core::interpolation::{render, InterpContext};
use crate::error::{AppError, AppResult};
use crate::security::blacklist;
use crate::state::AppState;
use crate::storage::db::DbPool;
use crate::storage::models::{CommandType, Param};

/// 把 serde_json::Value (期望是 object) 转成 HashMap<String, serde_json::Value>
fn json_env_to_map(v: &Option<serde_json::Value>) -> HashMap<String, serde_json::Value> {
    match v {
        Some(serde_json::Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => HashMap::new(),
    }
}

#[derive(Deserialize)]
pub struct RunCommandInput {
    pub command_id: String,
    /// 用户填写的参数: name -> value
    pub params: HashMap<String, serde_json::Value>,
    /// 是否跳过黑名单/危险确认（用户已二次确认）
    #[serde(default)]
    pub override_safety: bool,
}

#[derive(Serialize)]
pub struct RunCommandResponse {
    pub execution_id: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct CommandPreview {
    pub rendered: String,
    pub env: HashMap<String, String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<i64>,
    pub blacklisted: Option<String>,
    pub dangerous: bool,
    pub warnings: Vec<String>,
}

/// 预览命令：渲染模板、检查黑名单，返回最终命令给前端展示
#[tauri::command]
pub async fn preview_command(
    state: State<'_, AppState>,
    input: RunCommandInput,
) -> AppResult<CommandPreview> {
    let detail = fetch_command_detail(&state, &input.command_id).await?;
    let ctx = build_context(&input.params, &detail.params, &json_env_to_map(&detail.env))?;

    let rendered = render(&detail.template, &ctx)?;
    let cwd = detail.working_dir.as_ref().and_then(|t| render(t, &ctx).ok());
    let env_rendered = match &detail.env {
        Some(env_map) => {
            let mut out = HashMap::new();
            if let Some(obj) = env_map.as_object() {
                for (k, v) in obj {
                    if let Some(s) = v.as_str() {
                        if let Ok(r) = render(s, &ctx) {
                            out.insert(k.clone(), r);
                        }
                    } else {
                        out.insert(k.clone(), v.to_string());
                    }
                }
            }
            out
        }
        None => HashMap::new(),
    };

    let blacklisted = blacklist::check(&rendered).map(|h| h.rule.description.to_string());
    let dangerous = blacklist::is_dangerous_heuristic(&rendered);
    let mut warnings = Vec::new();
    if blacklisted.is_some() {
        warnings.push("命令命中黑名单规则，需要二次确认才能执行".to_string());
    } else if dangerous {
        warnings.push("命令包含危险操作（删除/格式化/强制推送等），建议二次确认".to_string());
    }

    Ok(CommandPreview {
        rendered,
        env: env_rendered,
        cwd,
        timeout_ms: detail.timeout_ms,
        blacklisted,
        dangerous,
        warnings,
    })
}

/// 执行命令
#[tauri::command]
pub async fn run_command(
    app: AppHandle,
    state: State<'_, AppState>,
    input: RunCommandInput,
) -> AppResult<RunCommandResponse> {
    let response = run_command_inner(
        app,
        state.db.clone(),
        state.execution_registry.clone(),
        input,
    )
    .await?;
    Ok(response)
}

/// 内部实现 — 也被 `replay_execution` 复用 (history.rs)
pub async fn run_command_inner(
    app: AppHandle,
    db: Arc<DbPool>,
    registry: Arc<crate::core::executor::ExecutionRegistry>,
    input: RunCommandInput,
) -> AppResult<RunCommandResponse> {
    let detail = fetch_command_detail_inner(&db, &input.command_id).await?;
    let ctx = build_context(&input.params, &detail.params, &json_env_to_map(&detail.env))?;
    let rendered = render(&detail.template, &ctx)?;

    if let Some(hit) = blacklist::check(&rendered) {
        if !input.override_safety {
            return Err(AppError::invalid(format!(
                "命令被黑名单拦截: {} (匹配: {})",
                hit.rule.description, hit.matched_text
            )));
        }
        tracing::warn!("用户 override 黑名单: {}", hit.rule.description);
    }

    let cwd = detail
        .working_dir
        .as_ref()
        .and_then(|t| render(t, &ctx).ok())
        .map(PathBuf::from);

    let env_rendered = if let Some(env_map) = &detail.env {
        if let Some(obj) = env_map.as_object() {
            let mut out = HashMap::new();
            for (k, v) in obj {
                if let Some(s) = v.as_str() {
                    if let Ok(r) = render(s, &ctx) {
                        out.insert(k.clone(), r);
                    }
                } else {
                    out.insert(k.clone(), v.to_string());
                }
            }
            out
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    let execution_id = uuid::Uuid::new_v4().to_string();
    let spec = ExecutionSpec {
        execution_id: execution_id.clone(),
        command_id: detail.id.clone(),
        command_name: detail.name.clone(),
        command_type: detail.command_type.as_str().to_string(),
        template: rendered,
        working_dir: cwd,
        env: env_rendered,
        timeout_ms: detail.timeout_ms.map(|t| t as u64),
        input_params: Some(serde_json::to_value(&input.params)?),
    };

    let result = executor::execute(app, db, registry, spec).await?;

    Ok(RunCommandResponse {
        execution_id,
        status: result.status.as_str().to_string(),
        exit_code: result.exit_code,
        duration_ms: result.duration_ms,
        stdout: result.stdout,
        stderr: result.stderr,
        error: result.error,
    })
}

/// 从历史里重放一条直接命令的入口（不走 State，避免 history.rs 借不到 State）
pub async fn replay_direct_command(
    app: AppHandle,
    db: Arc<DbPool>,
    registry: Arc<crate::core::executor::ExecutionRegistry>,
    command_id: String,
    params: std::collections::HashMap<String, serde_json::Value>,
) -> AppResult<String> {
    let input = RunCommandInput {
        command_id,
        params,
        override_safety: true,
    };
    let resp = run_command_inner(app, db, registry, input).await?;
    Ok(resp.execution_id)
}

#[tauri::command]
pub async fn cancel_execution(
    state: State<'_, AppState>,
    execution_id: String,
) -> AppResult<bool> {
    Ok(state.execution_registry.cancel(&execution_id).await)
}

#[derive(Serialize)]
pub struct ExecutionSummary {
    pub id: String,
    pub command_id: String,
    pub command_name: String,
    pub status: String,
    pub trigger: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn list_executions(
    state: State<'_, AppState>,
    limit: Option<i64>,
    status: Option<String>,
) -> AppResult<Vec<ExecutionSummary>> {
    let conn = state.db.get()?;
    let limit = limit.unwrap_or(50);

    let mut sql = String::from(
        "SELECT e.id, e.workflow_id, COALESCE(c.name, '?') as command_name, e.status, e.trigger, e.started_at, e.finished_at, e.duration_ms, e.error
         FROM executions e
         LEFT JOIN commands c ON c.id = e.workflow_id
         WHERE 1=1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(s) = status {
        sql.push_str(" AND e.status = ?");
        args.push(Box::new(s));
    }
    sql.push_str(" ORDER BY e.started_at DESC LIMIT ?");
    args.push(Box::new(limit));

    let arg_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*arg_refs, |r| {
        Ok(ExecutionSummary {
            id: r.get(0)?,
            command_id: r.get(1)?,
            command_name: r.get(2)?,
            status: r.get(3)?,
            trigger: r.get(4)?,
            started_at: r.get(5)?,
            finished_at: r.get(6)?,
            duration_ms: r.get(7)?,
            error: r.get(8)?,
        })
    })?;

    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct ExecutionDetail {
    #[serde(flatten)]
    pub summary: ExecutionSummary,
    pub input_params: Option<serde_json::Value>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

#[tauri::command]
pub async fn get_execution(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<ExecutionDetail> {
    let conn = state.db.get()?;

    let summary: ExecutionSummary = conn.query_row(
        "SELECT e.id, e.workflow_id, COALESCE(c.name, '?') as command_name, e.status, e.trigger, e.started_at, e.finished_at, e.duration_ms, e.error
         FROM executions e LEFT JOIN commands c ON c.id = e.workflow_id
         WHERE e.id = ?1",
        [&id],
        |r| {
            Ok(ExecutionSummary {
                id: r.get(0)?,
                command_id: r.get(1)?,
                command_name: r.get(2)?,
                status: r.get(3)?,
                trigger: r.get(4)?,
                started_at: r.get(5)?,
                finished_at: r.get(6)?,
                duration_ms: r.get(7)?,
                error: r.get(8)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found(format!("execution:{id}")),
        other => AppError::Database(other),
    })?;

    let (input_params, stdout, stderr) = conn
        .query_row(
            "SELECT e.input_params, n.stdout, n.stderr
             FROM executions e
             LEFT JOIN node_runs n ON n.execution_id = e.id
             WHERE e.id = ?1 LIMIT 1",
            [&id],
            |r| {
                let ip: Option<String> = r.get(0)?;
                let ip: Option<serde_json::Value> = ip.and_then(|s| serde_json::from_str(&s).ok());
                Ok((ip, r.get::<_, Option<String>>(1)?, r.get(2)?))
            },
        )
        .unwrap_or((None, None, None));

    Ok(ExecutionDetail {
        summary,
        input_params,
        stdout,
        stderr,
    })
}

// ==================== 内部 helper ====================

struct CommandForRun {
    id: String,
    name: String,
    command_type: CommandType,
    template: String,
    working_dir: Option<String>,
    env: Option<serde_json::Value>,
    timeout_ms: Option<i64>,
    params: Vec<Param>,
}

async fn fetch_command_detail(
    state: &State<'_, AppState>,
    command_id: &str,
) -> AppResult<CommandForRun> {
    fetch_command_detail_inner(&state.db, command_id).await
}

async fn fetch_command_detail_inner(
    db: &Arc<DbPool>,
    command_id: &str,
) -> AppResult<CommandForRun> {
    let conn = db.get()?;

    let (id, name, type_str, template, working_dir, env_str, timeout_ms): (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<i64>,
    ) = conn.query_row(
        "SELECT c.id, c.name, c.type, v.template, v.working_dir, v.env, v.timeout_ms
         FROM commands c
         JOIN command_versions v ON v.command_id = c.id AND v.version = c.current_ver
         WHERE c.id = ?1",
        [command_id],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found(format!("command:{command_id}")),
        other => AppError::Database(other),
    })?;

    let command_type = CommandType::parse(&type_str)
        .ok_or_else(|| AppError::other(format!("未知命令类型: {type_str}")))?;

    let env = env_str.and_then(|s| serde_json::from_str(&s).ok());

    // 参数
    let ver_id: i64 = conn.query_row(
        "SELECT id FROM command_versions WHERE command_id = ?1 AND version = (SELECT current_ver FROM commands WHERE id = ?1)",
        [command_id],
        |r| r.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, command_ver_id, name, label, type, required, default_value, options, validation, sensitive, description, sort_order
         FROM params WHERE command_ver_id = ?1 ORDER BY sort_order, name",
    )?;
    let params_iter = stmt.query_map([ver_id], |r| {
        Ok(Param {
            id: r.get(0)?,
            command_ver_id: r.get(1)?,
            name: r.get(2)?,
            label: r.get(3)?,
            param_type: r.get(4)?,
            required: r.get(5)?,
            default_value: r.get::<_, Option<String>>(6)?
                .and_then(|s| serde_json::from_str(&s).ok()),
            options: r.get::<_, Option<String>>(7)?
                .and_then(|s| serde_json::from_str(&s).ok()),
            validation: r.get::<_, Option<String>>(8)?
                .and_then(|s| serde_json::from_str(&s).ok()),
            sensitive: r.get(9)?,
            description: r.get(10)?,
            sort_order: r.get(11)?,
        })
    })?;
    let mut params = Vec::new();
    for p in params_iter {
        params.push(p?);
    }

    Ok(CommandForRun {
        id,
        name,
        command_type,
        template,
        working_dir,
        env,
        timeout_ms,
        params,
    })
}

fn build_context(
    input: &HashMap<String, serde_json::Value>,
    param_defs: &[Param],
    default_env: &HashMap<String, serde_json::Value>,
) -> AppResult<InterpContext> {
    let mut ctx = InterpContext::new();

    // 合并默认值
    for p in param_defs {
        let value = input
            .get(&p.name)
            .cloned()
            .or_else(|| p.default_value.clone());

        if let Some(v) = value {
            // 验证必填
            let s = match &v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if s.is_empty() && p.required {
                return Err(AppError::invalid(format!("参数 {} 必填", p.name)));
            }
            ctx.params.insert(p.name.clone(), s);
        } else if p.required {
            return Err(AppError::invalid(format!("参数 {} 必填", p.name)));
        }
    }

    // 系统环境
    for (k, v) in std::env::vars() {
        ctx.env.insert(k, v);
    }
    // 默认 env
    for (k, v) in default_env {
        if let Some(s) = v.as_str() {
            ctx.env.insert(k.clone(), s.to_string());
        }
    }

    Ok(ctx)
}
