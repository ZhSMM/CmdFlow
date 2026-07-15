//! 调度器
//!
//! 应用内调度循环：
//! - 启动时加载所有 enabled schedules
//! - 每 30s 扫一次，找出 next_run_at <= now 的
//! - 触发执行（复用 dag_executor）
//! - 更新 last_run_at 和 next_run_at
//!
//! OS 计划任务集成（可选）：通过 schtasks / launchd / systemd 注册。
//! 应用关闭时由 OS 任务兜底触发（通过 `cmdflow run --workflow <id>` 子命令）。

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tokio::time::interval;

use crate::error::{AppError, AppResult};
use crate::storage::db::DbPool;
use crate::storage::models::Workflow;

/// 调度记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRecord {
    pub id: String,
    pub workflow_id: String,
    pub cron_expr: String,
    pub timezone: String,
    pub enabled: bool,
    pub last_run_at: Option<i64>,
    pub next_run_at: Option<i64>,
    pub mode: String, // in_app | os_native | hybrid
    pub os_task_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleInput {
    pub workflow_id: String,
    pub cron_expr: String,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
    pub mode: Option<String>,
}

/// 把 5 段 cron 补全成 6 段 (cron crate 要求 6 段: sec min hour dom mon dow)
fn normalize_cron(cron_expr: &str) -> String {
    let parts: Vec<&str> = cron_expr.split_whitespace().collect();
    if parts.len() == 5 {
        format!("0 {}", cron_expr) // 加秒段
    } else {
        cron_expr.to_string()
    }
}

/// 计算 cron 表达式的下次触发时间
pub fn next_run(cron_expr: &str, after: DateTime<Utc>) -> AppResult<Option<DateTime<Utc>>> {
    let normalized = normalize_cron(cron_expr);
    let schedule = Schedule::from_str(&normalized)
        .map_err(|e| AppError::invalid(format!("无效 cron 表达式: {e}")))?;
    Ok(schedule.after(&after).next())
}

/// 校验 cron 表达式
pub fn validate_cron(cron_expr: &str) -> AppResult<()> {
    let normalized = normalize_cron(cron_expr);
    Schedule::from_str(&normalized)
        .map_err(|e| AppError::invalid(format!("无效 cron 表达式: {e}")))?;
    Ok(())
}

/// 调度器主循环
pub async fn run_scheduler_loop(app: AppHandle, db: Arc<DbPool>) {
    let mut tick = interval(Duration::from_secs(30));
    tick.tick().await; // 跳过首次立即 tick

    loop {
        tick.tick().await;
        if let Err(e) = scan_and_trigger(&app, &db).await {
            tracing::error!("调度器扫描失败: {e}");
        }
    }
}

/// 扫描待执行的调度
async fn scan_and_trigger(app: &AppHandle, db: &Arc<DbPool>) -> AppResult<()> {
    let now = Utc::now();
    let now_ts = now.timestamp();

    // 找出所有 enabled 且 next_run_at <= now 的 schedule
    let schedules: Vec<ScheduleRecord> = {
        let conn = db.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, workflow_id, cron_expr, timezone, enabled, last_run_at, next_run_at, mode, os_task_id, created_at
             FROM schedules WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?1",
        )?;
        let rows = stmt.query_map([now_ts], |r| {
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
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out
    };

    for sched in schedules {
        tracing::info!(
            "触发调度: schedule={}, workflow={}",
            sched.id,
            sched.workflow_id
        );
        if let Err(e) = trigger_one(app.clone(), db, &sched, now).await {
            tracing::error!("调度触发失败 {}: {e}", sched.id);
        }
    }

    // 同时更新所有 enabled schedule 的 next_run_at（如果 next_run_at 是过去的）
    refresh_next_runs(db).await?;

    Ok(())
}

async fn refresh_next_runs(db: &DbPool) -> AppResult<()> {
    let now = Utc::now();
    let now_ts = now.timestamp();

    let conn = db.get()?;
    let mut stmt =
        conn.prepare("SELECT id, cron_expr, next_run_at FROM schedules WHERE enabled = 1")?;
    let rows: Vec<(String, String, Option<i64>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);

    for (id, cron_expr, next_run_at) in rows {
        let needs_update = match next_run_at {
            None => true,
            Some(t) if t < now_ts => true,
            _ => false,
        };
        if needs_update {
            if let Some(next) = next_run(&cron_expr, now)? {
                let conn = db.get()?;
                conn.execute(
                    "UPDATE schedules SET next_run_at = ?1 WHERE id = ?2",
                    rusqlite::params![next.timestamp(), id],
                )?;
            }
        }
    }

    Ok(())
}

async fn trigger_one(
    app: AppHandle,
    db: &Arc<DbPool>,
    sched: &ScheduleRecord,
    now: DateTime<Utc>,
) -> AppResult<()> {
    // 加载工作流
    let workflow: Option<Workflow> = {
        let conn = db.get()?;
        conn.query_row(
            "SELECT id, name, description, enabled, trigger_type, created_at, updated_at
             FROM workflows WHERE id = ?1",
            [&sched.workflow_id],
            |r| {
                Ok(Workflow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    enabled: r.get(3)?,
                    trigger_type: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            },
        )
        .ok()
    };

    let workflow = match workflow {
        Some(w) if w.enabled => w,
        _ => {
            tracing::warn!("工作流 {} 不存在或已禁用，跳过调度", sched.workflow_id);
            return Ok(());
        }
    };

    // 加载节点和边
    let detail = crate::storage::models::WorkflowDetail::load_full(db, &workflow.id)?;
    let detail = match detail {
        Some(d) => d,
        None => return Ok(()),
    };

    // 注册执行 ID
    let execution_id = uuid::Uuid::new_v4().to_string();
    let registry = app
        .state::<crate::state::AppState>()
        .execution_registry
        .clone();
    let cancel = registry.register(execution_id.clone()).await;

    // 写 executions
    {
        let conn = db.get()?;
        conn.execute(
            "INSERT INTO executions (id, workflow_id, trigger, status, started_at) VALUES (?1, ?2, 'schedule', 'running', ?3)",
            rusqlite::params![execution_id, sched.workflow_id, now.timestamp()],
        )?;
    }

    // 更新 schedule 的 last_run_at
    if let Some(next) = next_run(&sched.cron_expr, now)? {
        let conn = db.get()?;
        conn.execute(
            "UPDATE schedules SET last_run_at = ?1, next_run_at = ?2 WHERE id = ?3",
            rusqlite::params![now.timestamp(), next.timestamp(), sched.id],
        )?;
    }

    // 异步执行
    let app2 = app.clone();
    let db2 = db.clone();
    let nodes = detail.nodes.clone();
    let edges = detail.edges.clone();
    let wf_id = workflow.id.clone();
    let wf_name = workflow.name.clone();
    let exec_id2 = execution_id.clone();

    tokio::spawn(async move {
        let res = crate::core::dag_executor::execute_workflow(
            app2.clone(),
            db2.clone(),
            exec_id2.clone(),
            wf_id,
            wf_name,
            nodes,
            edges,
            std::collections::HashMap::new(),
            cancel,
        )
        .await;

        let conn = match db2.get() {
            Ok(c) => c,
            Err(_) => return,
        };
        match res {
            Ok(r) => {
                let _ = conn.execute(
                    "UPDATE executions SET status = ?1, finished_at = ?2, duration_ms = ?3 WHERE id = ?4",
                    rusqlite::params![
                        r.status.as_str(),
                        Utc::now().timestamp(),
                        r.duration_ms as i64,
                        r.execution_id,
                    ],
                );
            }
            Err(e) => {
                let _ = conn.execute(
                    "UPDATE executions SET status = 'failed', finished_at = ?1, error = ?2 WHERE id = ?3",
                    rusqlite::params![
                        Utc::now().timestamp(),
                        e.to_string(),
                        exec_id2,
                    ],
                );
            }
        }
    });

    Ok(())
}

/// 启动调度器（在 AppState::init 后调）
///
/// 注意：Tauri 的 setup 回调是同步执行的，调用时主线程还没有进入
/// tokio runtime 上下文。直接 `tokio::spawn` 会 panic
/// （"there is no reactor running"）。改用 `tauri::async_runtime::spawn`，
/// 它会复用 Tauri 自带的 tokio 运行时，在任何位置都能正常 spawn。
pub fn start(app: AppHandle, db: Arc<DbPool>) {
    tauri::async_runtime::spawn(run_scheduler_loop(app, db));
    tracing::info!("调度器已启动");
}

/// 把 cron 表达式转可读描述（简单版）
pub fn describe_cron(cron_expr: &str) -> String {
    let parts: Vec<&str> = cron_expr.split_whitespace().collect();
    if parts.len() != 5 {
        return cron_expr.to_string();
    }
    let (min, hour, dom, mon, dow) = (parts[0], parts[1], parts[2], parts[3], parts[4]);

    // 简单模式匹配
    if min == "0" && hour == "*" && dom == "*" && mon == "*" && dow == "*" {
        return "每小时整点".to_string();
    }
    if min == "*/5" || min == "*/10" || min == "*/15" || min == "*/30" {
        return format!("每 {} 分钟", min.trim_start_matches("*/"));
    }
    if dom == "*" && mon == "*" && dow == "*" && min != "*" && hour != "*" {
        return format!(
            "每天 {:02}:{:02}",
            hour.parse::<u32>().unwrap_or(0),
            min.parse::<u32>().unwrap_or(0)
        );
    }
    cron_expr.to_string()
}

#[allow(dead_code)]
fn dummy_for_compile(_t: DateTime<Utc>) -> i64 {
    _t.timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Timelike, TimeZone};

    #[test]
    fn test_cron_validation() {
        // 接受 5 段 (标准) 或 6 段
        assert!(validate_cron("0 2 * * *").is_ok());
        assert!(validate_cron("*/5 * * * *").is_ok());
        assert!(validate_cron("0 0 2 * * *").is_ok());
        assert!(validate_cron("invalid").is_err());
    }

    #[test]
    fn test_next_run() {
        let after = Utc.with_ymd_and_hms(2026, 7, 13, 10, 0, 0).unwrap();
        let next = next_run("0 12 * * *", after).unwrap().unwrap();
        assert_eq!(next.hour(), 12);
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn test_describe_cron() {
        assert_eq!(describe_cron("0 * * * *"), "每小时整点");
        assert_eq!(describe_cron("*/15 * * * *"), "每 15 分钟");
        assert_eq!(describe_cron("30 9 * * *"), "每天 09:30");
    }
}
