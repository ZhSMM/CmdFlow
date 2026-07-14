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

    let mut sql = String::from(
        "SELECT e.id, e.workflow_id, COALESCE(w.name, '?') as workflow_name, e.trigger, e.status,
                e.started_at, e.finished_at, e.duration_ms, e.error
         FROM executions e LEFT JOIN workflows w ON w.id = e.workflow_id WHERE 1=1",
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
        "SELECT e.id, e.workflow_id, COALESCE(w.name, '?') as workflow_name, e.trigger, e.status,
                e.started_at, e.finished_at, e.duration_ms, e.error
         FROM executions e LEFT JOIN workflows w ON w.id = e.workflow_id
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

    let (input_params_str,): (Option<String>,) = conn.query_row(
        "SELECT input_params FROM executions WHERE id = ?1",
        [&id],
        |r| Ok((r.get::<_, Option<String>>(0)?,)),
    ).unwrap_or((None,));
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

    // 2. 用相同 workflow + params 触发
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
