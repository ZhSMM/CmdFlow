//! 调度 IPC

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::task;

use crate::core::scheduler::{self, ScheduleInput, ScheduleRecord};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Serialize)]
pub struct ScheduleWithWorkflow {
    #[serde(flatten)]
    pub schedule: ScheduleRecord,
    pub workflow_name: String,
    pub workflow_enabled: bool,
}

#[tauri::command]
pub async fn list_schedules(state: State<'_, AppState>) -> AppResult<Vec<ScheduleWithWorkflow>> {
    let conn = state.db.get()?;
    let mut stmt = conn.prepare(
        "SELECT s.id, s.workflow_id, s.cron_expr, s.timezone, s.enabled, s.last_run_at, s.next_run_at, s.mode, s.os_task_id, s.created_at,
                w.name, w.enabled
         FROM schedules s LEFT JOIN workflows w ON w.id = s.workflow_id
         ORDER BY s.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ScheduleWithWorkflow {
            schedule: ScheduleRecord {
                id: r.get(0)?,
                workflow_id: r.get(1)?,
                cron_expr: r.get(2)?,
                timezone: r.get(3)?,
                enabled: r.get(4)?,
                last_run_at: r.get(5)?,
                next_run_at: r.get(6)?,
                mode: r.get(7)?,
                os_task_id: r.get(8)?,
                created_at: r.get(9)?,
            },
            workflow_name: r.get::<_, Option<String>>(10)?.unwrap_or("?".into()),
            workflow_enabled: r.get::<_, Option<bool>>(11)?.unwrap_or(false),
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[tauri::command]
pub async fn create_schedule(
    state: State<'_, AppState>,
    input: ScheduleInput,
) -> AppResult<String> {
    // 校验 cron
    scheduler::validate_cron(&input.cron_expr)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now_ts = Utc::now().timestamp();
    let next_at = scheduler::next_run(&input.cron_expr, Utc::now())?.map(|d| d.timestamp());

    let conn = state.db.get()?;
    conn.execute(
        "INSERT INTO schedules (id, workflow_id, cron_expr, timezone, enabled, next_run_at, mode, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            input.workflow_id,
            input.cron_expr,
            input.timezone.as_deref().unwrap_or("local"),
            input.enabled.unwrap_or(true) as i32,
            next_at,
            input.mode.as_deref().unwrap_or("in_app"),
            now_ts,
        ],
    )?;

    // 触发 OS 任务注册
    if let Some(mode) = &input.mode {
        if mode != "in_app" {
            register_os_task(&id, &input).await.ok();
        }
    }

    Ok(id)
}

#[derive(Deserialize)]
pub struct UpdateScheduleInput {
    pub id: String,
    pub cron_expr: Option<String>,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
    pub mode: Option<String>,
}

#[tauri::command]
pub async fn update_schedule(
    state: State<'_, AppState>,
    input: UpdateScheduleInput,
) -> AppResult<()> {
    let conn = state.db.get()?;

    if let Some(cron) = &input.cron_expr {
        scheduler::validate_cron(cron)?;
        let next = scheduler::next_run(cron, Utc::now())?.map(|d| d.timestamp());
        conn.execute(
            "UPDATE schedules SET cron_expr = ?1, next_run_at = ?2 WHERE id = ?3",
            params![cron, next, input.id],
        )?;
    }
    if let Some(tz) = &input.timezone {
        conn.execute(
            "UPDATE schedules SET timezone = ?1 WHERE id = ?2",
            params![tz, input.id],
        )?;
    }
    if let Some(en) = input.enabled {
        conn.execute(
            "UPDATE schedules SET enabled = ?1 WHERE id = ?2",
            params![en as i32, input.id],
        )?;
    }
    if let Some(mode) = &input.mode {
        conn.execute(
            "UPDATE schedules SET mode = ?1 WHERE id = ?2",
            params![mode, input.id],
        )?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_schedule(state: State<'_, AppState>, id: String) -> AppResult<()> {
    // 先 unregister OS 任务
    unregister_os_task(&id).await.ok();
    let conn = state.db.get()?;
    conn.execute("DELETE FROM schedules WHERE id = ?1", [&id])?;
    Ok(())
}

#[tauri::command]
pub async fn trigger_schedule_now(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<String> {
    let sched: ScheduleRecord = {
        let conn = state.db.get()?;
        conn.query_row(
            "SELECT id, workflow_id, cron_expr, timezone, enabled, last_run_at, next_run_at, mode, os_task_id, created_at
             FROM schedules WHERE id = ?1",
            [&id],
            |r| {
                Ok(ScheduleRecord {
                    id: r.get(0)?,
                    workflow_id: r.get(1)?,
                    cron_expr: r.get(2)?,
                    timezone: r.get(3)?,
                    enabled: r.get(4)?,
                    last_run_at: r.get(5)?,
                    next_run_at: r.get(6)?,
                    mode: r.get(7)?,
                    os_task_id: r.get(8)?,
                    created_at: r.get(9)?,
                })
            },
        )
        .map_err(|_| AppError::not_found(format!("schedule:{id}")))?
    };

    let execution_id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();

    {
        let conn = state.db.get()?;
        conn.execute(
            "INSERT INTO executions (id, workflow_id, trigger, status, started_at) VALUES (?1, ?2, 'manual', 'running', ?3)",
            params![execution_id, sched.workflow_id, now.timestamp()],
        )?;
    }

    let detail = crate::storage::models::WorkflowDetail::load_full(&state.db, &sched.workflow_id)?;
    let detail =
        detail.ok_or_else(|| AppError::not_found(format!("workflow:{}", sched.workflow_id)))?;

    let registry = state.execution_registry.clone();
    let cancel = registry.register(execution_id.clone()).await;
    let app2 = app.clone();
    let db2 = state.db.clone();
    let db3 = state.db.clone();
    let nodes = detail.nodes.clone();
    let edges = detail.edges.clone();
    let wf_id = sched.workflow_id.clone();
    let wf_name = detail.workflow.name.clone();
    let exec_id3 = execution_id.clone();

    task::spawn(async move {
        let res = crate::core::dag_executor::execute_workflow(
            app2,
            db2,
            exec_id3.clone(),
            wf_id,
            wf_name,
            nodes,
            edges,
            std::collections::HashMap::new(),
            cancel,
        )
        .await;
        if let Ok(conn) = db3.get() {
            match res {
                Ok(r) => {
                    let _ = conn.execute(
                        "UPDATE executions SET status = ?1, finished_at = ?2, duration_ms = ?3 WHERE id = ?4",
                        params![r.status.as_str(), Utc::now().timestamp(), r.duration_ms as i64, r.execution_id],
                    );
                }
                Err(e) => {
                    let _ = conn.execute(
                        "UPDATE executions SET status = 'failed', finished_at = ?1, error = ?2 WHERE id = ?3",
                        params![Utc::now().timestamp(), e.to_string(), exec_id3],
                    );
                }
            }
        }
    });

    Ok(execution_id)
}

#[tauri::command]
pub fn validate_cron_expression(expr: String) -> AppResult<()> {
    scheduler::validate_cron(&expr)
}

#[tauri::command]
pub fn describe_cron_expression(expr: String) -> String {
    scheduler::describe_cron(&expr)
}

// ==================== OS 任务集成 ====================

async fn register_os_task(schedule_id: &str, input: &ScheduleInput) -> AppResult<()> {
    #[cfg(windows)]
    {
        use std::process::Command;
        let task_name = format!("CmdFlow-{}", &schedule_id[..8.min(schedule_id.len())]);
        let cron_desc = scheduler::describe_cron(&input.cron_expr);
        // 简化: 每天固定时间
        // 实际应解析 cron 后填 /sc once /st HH:MM
        // Phase 3 简化: 用 /sc daily 兜底
        let exe = std::env::current_exe()
            .map_err(|e| AppError::other(format!("获取 exe 路径失败: {e}")))?;
        let task_args = format!(
            "\"{}\" run --workflow {} --scheduled",
            exe.display(),
            input.workflow_id,
        );

        let output = Command::new("schtasks")
            .args([
                "/Create", "/SC", "ONCE", "/TN", &task_name, "/TR", &task_args, "/ST", "00:00",
                "/F",
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                tracing::info!("OS 任务已注册: {}", task_name);
                let conn_str = "";
                let _ = conn_str; // OS 任务 ID 写库
            }
            _ => {
                tracing::warn!(
                    "OS 任务注册失败 (Phase 3 简化版,任务: {} 时间: {})",
                    task_name,
                    cron_desc
                );
            }
        }
    }
    #[cfg(not(windows))]
    {
        tracing::info!("非 Windows 平台,跳过 OS 任务注册");
    }
    Ok(())
}

async fn unregister_os_task(schedule_id: &str) -> AppResult<()> {
    #[cfg(windows)]
    {
        use std::process::Command;
        let task_name = format!("CmdFlow-{}", &schedule_id[..8.min(schedule_id.len())]);
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", &task_name, "/F"])
            .output();
        tracing::info!("OS 任务已尝试注销: {}", task_name);
    }
    Ok(())
}
