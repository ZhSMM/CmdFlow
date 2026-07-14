//! 子进程执行引擎
//!
//! 负责：
//! 1. 启动 OS 子进程
//! 2. 流式读取 stdout/stderr
//! 3. 超时控制
//! 4. 取消
//! 5. 推送事件给前端
//! 6. 持久化执行历史

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::params;
use tauri::AppHandle;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::core::events::{
    emit, NodeStatus, RunEvent, StreamKind,
};
use crate::error::AppResult;
use crate::storage::db::DbPool;

/// 执行规格
#[derive(Debug, Clone)]
pub struct ExecutionSpec {
    pub execution_id: String,
    pub command_id: String,
    pub command_name: String,
    pub command_type: String, // cmd / pwsh / python / node / bash
    pub template: String, // 已渲染完的最终命令
    pub working_dir: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub timeout_ms: Option<u64>,
    pub input_params: Option<serde_json::Value>,
}

/// 执行结果
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub execution_id: String,
    pub exit_code: Option<i32>,
    pub status: NodeStatus,
    pub duration_ms: u64,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

/// 全局执行句柄表，用于取消
#[derive(Default)]
pub struct ExecutionRegistry {
    handles: Mutex<HashMap<String, CancellationToken>>,
}

impl ExecutionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(&self, id: String) -> CancellationToken {
        let token = CancellationToken::new();
        self.handles.lock().await.insert(id, token.clone());
        token
    }

    pub async fn cancel(&self, id: &str) -> bool {
        if let Some(token) = self.handles.lock().await.get(id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub async fn unregister(&self, id: &str) {
        self.handles.lock().await.remove(id);
    }
}

/// 启动一个执行
pub async fn execute(
    app: AppHandle,
    db: Arc<DbPool>,
    registry: Arc<ExecutionRegistry>,
    spec: ExecutionSpec,
) -> AppResult<ExecutionResult> {
    let started_at = chrono::Utc::now().timestamp();

    // 1. 写 executions 表
    insert_execution(&db, &spec, started_at).await?;

    // 2. 注册 cancel handle
    let cancel = registry.register(spec.execution_id.clone()).await;

    // 3. 推开始事件
    emit(
        &app,
        &RunEvent::ExecutionStarted {
            execution_id: spec.execution_id.clone(),
            command_id: spec.command_id.clone(),
            command_name: spec.command_name.clone(),
            started_at,
        },
    );
    emit(
        &app,
        &RunEvent::NodeStarted {
            execution_id: spec.execution_id.clone(),
            node_id: spec.command_id.clone(),
            node_name: spec.command_name.clone(),
            started_at,
        },
    );

    // 4. 启动子进程
    let (program, args) = build_command(&spec.command_type, &spec.template);
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(cwd) = &spec.working_dir {
        cmd.current_dir(cwd);
    }
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }

    // Windows: 创建新进程组，便于整体 kill
    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    let spawn_result = cmd.spawn();
    let mut child = match spawn_result {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("[executor] spawn failed exec_id={} err={}", spec.execution_id, e);
            let result = ExecutionResult {
                execution_id: spec.execution_id.clone(),
                exit_code: None,
                status: NodeStatus::Failed,
                duration_ms: 0,
                stdout: String::new(),
                stderr: String::new(),
                error: Some(format!("启动子进程失败: {e}")),
            };
            finalize(&app, &db, &spec, &result, started_at).await?;
            registry.unregister(&spec.execution_id).await;
            return Ok(result);
        }
    };

    let stdout = child.stdout.take().expect("已设置为 piped");
    let stderr = child.stderr.take().expect("已设置为 piped");

    // 5. 启动流读取任务
    let stdout_buf = Arc::new(Mutex::new(String::new()));
    let stderr_buf = Arc::new(Mutex::new(String::new()));

    let stdout_task = spawn_log_reader(
        app.clone(),
        spec.execution_id.clone(),
        spec.command_id.clone(),
        StreamKind::Stdout,
        BufReader::new(stdout),
        stdout_buf.clone(),
    );
    let stderr_task = spawn_log_reader(
        app.clone(),
        spec.execution_id.clone(),
        spec.command_id.clone(),
        StreamKind::Stderr,
        BufReader::new(stderr),
        stderr_buf.clone(),
    );

    // 6. 等待完成 / 超时 / 取消
    let start = Instant::now();
    let timeout_duration = spec.timeout_ms.map(Duration::from_millis);
    let wait_result = if let Some(dur) = timeout_duration {
        tokio::select! {
            r = child.wait() => Ok(r),
            _ = cancel.cancelled() => Err("cancelled".to_string()),
            _ = tokio::time::sleep(dur) => Err("timeout".to_string()),
        }
    } else {
        tokio::select! {
            r = child.wait() => Ok(r),
            _ = cancel.cancelled() => Err("cancelled".to_string()),
        }
    };

    let duration_ms = start.elapsed().as_millis() as u64;

    // 7. 等待流读完
    let _ = stdout_task.await;
    let _ = stderr_task.await;
    let stdout_text = stdout_buf.lock().await.clone();
    let stderr_text = stderr_buf.lock().await.clone();

    // 8. 构造结果
    let (status, exit_code, error) = match wait_result {
        Ok(Ok(es)) => {
            let code = es.code();
            if code == Some(0) {
                (NodeStatus::Success, code, None)
            } else {
                (
                    NodeStatus::Failed,
                    code,
                    Some(format!("退出码: {:?}", code)),
                )
            }
        }
        Ok(Err(e)) => (NodeStatus::Failed, None, Some(format!("wait 失败: {e}"))),
        Err(reason) if reason == "cancelled" => {
            // 杀进程
            let _ = child.start_kill();
            (NodeStatus::Cancelled, None, Some("用户取消".to_string()))
        }
        Err(reason) if reason == "timeout" => {
            let _ = child.start_kill();
            (NodeStatus::Timeout, None, Some("执行超时".to_string()))
        }
        Err(other) => (NodeStatus::Failed, None, Some(other)),
    };

    let result = ExecutionResult {
        execution_id: spec.execution_id.clone(),
        exit_code,
        status,
        duration_ms,
        stdout: stdout_text,
        stderr: stderr_text,
        error,
    };

    // 9. 收尾
    finalize(&app, &db, &spec, &result, started_at).await?;
    registry.unregister(&spec.execution_id).await;

    Ok(result)
}

fn build_command(command_type: &str, template: &str) -> (String, Vec<String>) {
    match command_type {
        "cmd" => {
            // cmd /c "command"
            ("cmd".to_string(), vec!["/c".to_string(), template.to_string()])
        }
        "pwsh" => {
            ("pwsh".to_string(), vec!["-NoProfile".to_string(), "-Command".to_string(), template.to_string()])
        }
        "powershell" => {
            ("powershell".to_string(), vec!["-NoProfile".to_string(), "-Command".to_string(), template.to_string()])
        }
        "python" => {
            ("python".to_string(), vec!["-c".to_string(), template.to_string()])
        }
        "node" => {
            ("node".to_string(), vec!["-e".to_string(), template.to_string()])
        }
        "bash" => {
            ("bash".to_string(), vec!["-c".to_string(), template.to_string()])
        }
        // 自定义/未识别:直接当作可执行文件名
        other => (other.to_string(), vec![template.to_string()]),
    }
}

fn spawn_log_reader<R>(
    app: AppHandle,
    execution_id: String,
    node_id: String,
    stream: StreamKind,
    reader: R,
    buf: Arc<Mutex<String>>,
) -> tokio::task::JoinHandle<()>
where
    R: AsyncBufRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = reader.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    buf.lock().await.push_str(&line);
                    buf.lock().await.push('\n');
                    emit(
                        &app,
                        &RunEvent::NodeLog {
                            execution_id: execution_id.clone(),
                            node_id: node_id.clone(),
                            stream,
                            content: format!("{line}\n"),
                            ts: chrono::Utc::now().timestamp_millis(),
                        },
                    );
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!("读取流失败: {e}");
                    break;
                }
            }
        }
    })
}

async fn insert_execution(
    db: &DbPool,
    spec: &ExecutionSpec,
    started_at: i64,
) -> AppResult<()> {
    let conn = db.get()?;
    let input_params = spec
        .input_params
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    conn.execute(
        "INSERT INTO executions (id, workflow_id, trigger, status, started_at, input_params)
         VALUES (?1, ?2, 'manual', 'running', ?3, ?4)",
        params![
            spec.execution_id,
            spec.command_id, // Phase 1 复用 workflow_id 字段存 command_id
            started_at,
            input_params,
        ],
    )?;
    Ok(())
}

async fn finalize(
    app: &AppHandle,
    db: &DbPool,
    spec: &ExecutionSpec,
    result: &ExecutionResult,
    started_at: i64,
) -> AppResult<()> {
    let finished_at = chrono::Utc::now().timestamp();

    // 更新 executions
    {
        let conn = db.get()?;
        conn.execute(
            "UPDATE executions
             SET status = ?1, finished_at = ?2, duration_ms = ?3, error = ?4
             WHERE id = ?5",
            params![
                result.status.as_str(),
                finished_at,
                result.duration_ms as i64,
                result.error,
                result.execution_id,
            ],
        )?;
    }

    // 写 node_runs
    {
        let conn = db.get()?;
        let node_run_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO node_runs (id, execution_id, node_id, status, started_at, finished_at, duration_ms, exit_code, stdout, stderr, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                node_run_id,
                result.execution_id,
                spec.command_id,
                result.status.as_str(),
                started_at,
                finished_at,
                result.duration_ms as i64,
                result.exit_code,
                result.stdout,
                result.stderr,
                result.error,
            ],
        )?;
    }

    // 发完成事件
    emit(
        app,
        &RunEvent::NodeFinished {
            execution_id: result.execution_id.clone(),
            node_id: spec.command_id.clone(),
            exit_code: result.exit_code,
            status: result.status,
            duration_ms: result.duration_ms,
        },
    );
    emit(
        app,
        &RunEvent::ExecutionFinished {
            execution_id: result.execution_id.clone(),
            status: result.status,
            duration_ms: result.duration_ms,
            error: result.error.clone(),
        },
    );

    Ok(())
}
