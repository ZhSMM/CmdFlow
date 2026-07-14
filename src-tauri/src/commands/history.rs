//! 执行历史 IPC

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Serialize)]
pub struct HistorySummary {
    pub id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub trigger: String,
    pub status: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct NodeRunInfo {
    pub id: String,
    pub node_id: String,
    pub status: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub output_data: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct HistoryDetail {
    #[serde(flatten)]
    pub summary: HistorySummary,
    pub input_params: Option<serde_json::Value>,
    pub rendered_template: Option<String>,
    pub node_runs: Vec<NodeRunInfo>,
}

#[tauri::command]
pub async fn list_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
    workflow_id: Option<String>,
    status: Option<String>,
) -> AppResult<Vec<HistorySummary>> {
    let conn = state.db.get()?;
    let limit = limit.unwrap_or(50);

    // workflow_id 字段同时存了 workflow id 和 command id (Phase 1 设计)
    // 用 COALESCE 优先拿 commands.name，拿不到再退到 workflows.name
    let mut sql = String::from(
        "SELECT e.id, e.workflow_id,
                COALESCE(c.name, w.name, '?') as workflow_name,
                e.trigger, e.status,
                e.started_at, e.finished_at, e.duration_ms, e.error
         FROM executions e
         LEFT JOIN workflows w ON w.id = e.workflow_id
         LEFT JOIN commands  c ON c.id = e.workflow_id
         WHERE 1=1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(wid) = workflow_id {
        sql.push_str(" AND e.workflow_id = ?");
        args.push(Box::new(wid));
    }
    if let Some(s) = status {
        sql.push_str(" AND e.status = ?");
        args.push(Box::new(s));
    }
    sql.push_str(" ORDER BY COALESCE(e.started_at, 0) DESC LIMIT ?");
    args.push(Box::new(limit));
    let arg_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*arg_refs, |r| {
        Ok(HistorySummary {
            id: r.get(0)?,
            workflow_id: r.get(1)?,
            workflow_name: r.get(2)?,
            trigger: r.get(3)?,
            status: r.get(4)?,
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

#[tauri::command]
pub async fn get_history_detail(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<HistoryDetail> {
    let conn = state.db.get()?;

    let summary: HistorySummary = conn.query_row(
        "SELECT e.id, e.workflow_id,
                COALESCE(c.name, w.name, '?') as workflow_name,
                e.trigger, e.status,
                e.started_at, e.finished_at, e.duration_ms, e.error
         FROM executions e
         LEFT JOIN workflows w ON w.id = e.workflow_id
         LEFT JOIN commands  c ON c.id = e.workflow_id
         WHERE e.id = ?1",
        [&id],
        |r| Ok(HistorySummary {
            id: r.get(0)?,
            workflow_id: r.get(1)?,
            workflow_name: r.get(2)?,
            trigger: r.get(3)?,
            status: r.get(4)?,
            started_at: r.get(5)?,
            finished_at: r.get(6)?,
            duration_ms: r.get(7)?,
            error: r.get(8)?,
        }),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found(format!("execution:{id}")),
        other => AppError::Database(other),
    })?;

    let (input_params_str, rendered_template): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT input_params, rendered_template FROM executions WHERE id = ?1",
            [&id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .unwrap_or((None, None));
    let input_params = input_params_str.and_then(|s| serde_json::from_str(&s).ok());

    let mut nstmt = conn.prepare(
        "SELECT id, node_id, status, started_at, finished_at, duration_ms, exit_code, stdout, stderr, output_data, error
         FROM node_runs WHERE execution_id = ?1 ORDER BY started_at",
    )?;
    let node_runs: Vec<NodeRunInfo> = nstmt
        .query_map([&id], |r| {
            let od: Option<String> = r.get(9)?;
            let od: Option<serde_json::Value> = od.and_then(|s| serde_json::from_str(&s).ok());
            Ok(NodeRunInfo {
                id: r.get(0)?,
                node_id: r.get(1)?,
                status: r.get(2)?,
                started_at: r.get(3)?,
                finished_at: r.get(4)?,
                duration_ms: r.get(5)?,
                exit_code: r.get(6)?,
                stdout: r.get(7)?,
                stderr: r.get(8)?,
                output_data: od,
                error: r.get(10)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    Ok(HistoryDetail {
        summary,
        input_params,
        rendered_template,
        node_runs,
    })
}

#[tauri::command]
pub async fn replay_execution(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    execution_id: String,
) -> AppResult<String> {
    // 1. 读历史 execution
    let conn = state.db.get()?;
    let (workflow_id, input_params_str,): (String, Option<String>) = conn.query_row(
        "SELECT workflow_id, input_params FROM executions WHERE id = ?1",
        [&execution_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).map_err(|_| AppError::not_found(format!("execution:{execution_id}")))?;
    drop(conn);

    // workflow_id 字段既存 workflow 也存 command (Phase 1 设计)
    // 先看 commands 表，能找到就走「直接重放命令」路径
    let conn = state.db.get()?;
    let is_command: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM commands WHERE id = ?1)",
            [&workflow_id],
            |r| r.get(0),
        )
        .unwrap_or(false);
    drop(conn);

    if is_command {
        // 直接命令重放 — 走 executor 的同一路径
        let input_params_value: serde_json::Value = input_params_str
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        let input_params_map: std::collections::HashMap<String, serde_json::Value> =
            match input_params_value {
                serde_json::Value::Object(m) => m.into_iter().collect(),
                _ => std::collections::HashMap::new(),
            };
        let result = crate::commands::execution::replay_direct_command(
            app,
            state.db.clone(),
            state.execution_registry.clone(),
            workflow_id,
            input_params_map,
        )
        .await?;
        return Ok(result);
    }

    // 2. 当作 workflow 走原路径
    let detail = crate::storage::models::WorkflowDetail::load_full(&state.db, &workflow_id)?
        .ok_or_else(|| AppError::not_found(format!("workflow:{workflow_id}")))?;

    let new_execution_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    {
        let conn = state.db.get()?;
        conn.execute(
            "INSERT INTO executions (id, workflow_id, trigger, status, started_at, input_params) VALUES (?1, ?2, 'manual', 'running', ?3, ?4)",
            rusqlite::params![new_execution_id, workflow_id, now, input_params_str],
        )?;
    }

    let input_params: std::collections::HashMap<String, String> = input_params_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let registry = state.execution_registry.clone();
    let cancel = registry.register(new_execution_id.clone()).await;
    let app2 = app.clone();
    let db2 = state.db.clone();
    let db3 = state.db.clone();
    let nodes = detail.nodes.clone();
    let edges = detail.edges.clone();
    let wf_id = workflow_id.clone();
    let wf_name = detail.workflow.name.clone();
    let exec_id3 = new_execution_id.clone();

    tokio::spawn(async move {
        let res = crate::core::dag_executor::execute_workflow(
            app2, db2, exec_id3.clone(), wf_id, wf_name, nodes, edges, input_params, cancel,
        ).await;
        if let Ok(conn) = db3.get() {
            match res {
                Ok(r) => {
                    let _ = conn.execute(
                        "UPDATE executions SET status = ?1, finished_at = ?2, duration_ms = ?3 WHERE id = ?4",
                        rusqlite::params![r.status.as_str(), chrono::Utc::now().timestamp(), r.duration_ms as i64, r.execution_id],
                    );
                }
                Err(e) => {
                    let _ = conn.execute(
                        "UPDATE executions SET status = 'failed', finished_at = ?1, error = ?2 WHERE id = ?3",
                        rusqlite::params![chrono::Utc::now().timestamp(), e.to_string(), exec_id3],
                    );
                }
            }
        }
    });

    Ok(new_execution_id)
}

// ==================== Phase 6: 增强 ====================

#[derive(Serialize)]
pub struct HistoryStats {
    pub total: i64,
    pub success: i64,
    pub failed: i64,
    pub running: i64,
    pub avg_duration_ms: i64,
    pub last_24h_count: i64,
    pub by_status: Vec<StatusBucket>,
}

#[derive(Serialize)]
pub struct StatusBucket {
    pub status: String,
    pub count: i64,
}

#[tauri::command]
pub async fn get_history_stats(state: State<'_, AppState>) -> AppResult<HistoryStats> {
    let conn = state.db.get()?;

    let (total, success, failed, running, avg_duration, last_24h): (i64, i64, i64, i64, i64, i64) = conn
        .query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'running' THEN 1 ELSE 0 END), 0),
                COALESCE(AVG(duration_ms), 0),
                COALESCE(SUM(CASE WHEN started_at > ?1 THEN 1 ELSE 0 END), 0)
             FROM executions",
            rusqlite::params![chrono::Utc::now().timestamp() - 86400],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )?;

    let mut stmt = conn.prepare(
        "SELECT status, COUNT(*) as c FROM executions GROUP BY status ORDER BY c DESC",
    )?;
    let by_status: Vec<StatusBucket> = stmt
        .query_map([], |r| Ok(StatusBucket { status: r.get(0)?, count: r.get(1)? }))?
        .filter_map(Result::ok)
        .collect();

    Ok(HistoryStats {
        total,
        success,
        failed,
        running,
        avg_duration_ms: avg_duration,
        last_24h_count: last_24h,
        by_status,
    })
}

/// 删除单条历史 (级联删除 node_runs 和 run_logs)
#[tauri::command]
pub async fn delete_history(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.get()?;
    // FK 关系: node_runs.execution_id → executions.id; run_logs.node_run_id → node_runs.id
    // 先删 run_logs 再 node_runs 再 executions
    conn.execute(
        "DELETE FROM run_logs WHERE node_run_id IN (SELECT id FROM node_runs WHERE execution_id = ?1)",
        [&id],
    )?;
    conn.execute("DELETE FROM node_runs WHERE execution_id = ?1", [&id])?;
    conn.execute("DELETE FROM executions WHERE id = ?1", [&id])?;
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct ClearHistoryInput {
    /// 只清某种状态
    pub status: Option<String>,
    /// 只清某个工作流
    pub workflow_id: Option<String>,
    /// 清比这早的 (unix ts 秒)
    pub before_ts: Option<i64>,
    /// 真正删除 (false = 返回会被删除的数量)
    pub dry_run: Option<bool>,
}

#[derive(Serialize)]
pub struct ClearHistoryResult {
    pub deleted: usize,
}

#[tauri::command]
pub async fn clear_history(
    state: State<'_, AppState>,
    input: ClearHistoryInput,
) -> AppResult<ClearHistoryResult> {
    let conn = state.db.get()?;

    // 拼条件
    let mut where_clause = String::from(" WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(s) = &input.status {
        where_clause.push_str(" AND status = ?");
        args.push(Box::new(s.clone()));
    }
    if let Some(wid) = &input.workflow_id {
        where_clause.push_str(" AND workflow_id = ?");
        args.push(Box::new(wid.clone()));
    }
    if let Some(ts) = input.before_ts {
        where_clause.push_str(" AND COALESCE(started_at, 0) < ?");
        args.push(Box::new(ts));
    }

    // 先 count
    let count_sql = format!("SELECT COUNT(*) FROM executions{}", where_clause);
    let arg_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
    let count: i64 = conn.query_row(&count_sql, &*arg_refs, |r| r.get(0))?;
    let count = count as usize;

    if !input.dry_run.unwrap_or(false) && count > 0 {
        // 先删 run_logs
        let log_sql = format!(
            "DELETE FROM run_logs WHERE node_run_id IN (SELECT id FROM node_runs WHERE execution_id IN (SELECT id FROM executions{}))",
            where_clause
        );
        conn.execute(&log_sql, &*arg_refs)?;

        // 再删 node_runs
        let nr_sql = format!(
            "DELETE FROM node_runs WHERE execution_id IN (SELECT id FROM executions{})",
            where_clause
        );
        conn.execute(&nr_sql, &*arg_refs)?;

        // 最后删 executions
        let del_sql = format!("DELETE FROM executions{}", where_clause);
        conn.execute(&del_sql, &*arg_refs)?;
    }

    Ok(ClearHistoryResult { deleted: count })
}

/// 按名称/ID 模糊搜索历史
#[tauri::command]
pub async fn search_history(
    state: State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> AppResult<Vec<HistorySummary>> {
    let conn = state.db.get()?;
    let limit = limit.unwrap_or(50);
    let pat = format!("%{}%", query);

    let mut stmt = conn.prepare(
        "SELECT e.id, e.workflow_id,
                COALESCE(c.name, w.name, '?') as workflow_name,
                e.trigger, e.status,
                e.started_at, e.finished_at, e.duration_ms, e.error
         FROM executions e
         LEFT JOIN workflows w ON w.id = e.workflow_id
         LEFT JOIN commands  c ON c.id = e.workflow_id
         WHERE e.id LIKE ?1
            OR e.workflow_id LIKE ?1
            OR COALESCE(c.name, w.name, '') LIKE ?1
            OR e.error LIKE ?1
         ORDER BY COALESCE(e.started_at, 0) DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![pat, limit], |r| {
        Ok(HistorySummary {
            id: r.get(0)?,
            workflow_id: r.get(1)?,
            workflow_name: r.get(2)?,
            trigger: r.get(3)?,
            status: r.get(4)?,
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
