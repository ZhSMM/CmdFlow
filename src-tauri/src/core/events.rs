//! 事件总线
//!
//! Tauri 2 通过 app.emit 推送事件给前端。
//! 前端用 listen('run-event', ...) 订阅。
//!
//! 事件统一带 execution_id，前端按执行 ID 路由到对应面板。

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// 执行过程中的事件
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunEvent {
    /// 执行开始
    ExecutionStarted {
        execution_id: String,
        command_id: String,
        command_name: String,
        started_at: i64,
    },
    /// 节点开始
    NodeStarted {
        execution_id: String,
        node_id: String,
        node_name: String,
        started_at: i64,
    },
    /// 节点输出日志（流式分片）
    NodeLog {
        execution_id: String,
        node_id: String,
        stream: StreamKind,
        content: String,
        ts: i64,
    },
    /// 节点结束
    NodeFinished {
        execution_id: String,
        node_id: String,
        exit_code: Option<i32>,
        status: NodeStatus,
        duration_ms: u64,
    },
    /// 执行结束
    ExecutionFinished {
        execution_id: String,
        status: NodeStatus,
        duration_ms: u64,
        error: Option<String>,
    },
    /// 取消请求
    ExecutionCancelled {
        execution_id: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    Stdout,
    Stderr,
    System,
}

impl StreamKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StreamKind::Stdout => "stdout",
            StreamKind::Stderr => "stderr",
            StreamKind::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    #[default]
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
    Timeout,
}

impl NodeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeStatus::Pending => "pending",
            NodeStatus::Running => "running",
            NodeStatus::Success => "success",
            NodeStatus::Failed => "failed",
            NodeStatus::Cancelled => "cancelled",
            NodeStatus::Timeout => "timeout",
        }
    }
}

pub const RUN_EVENT: &str = "run-event";

/// 推送事件给前端
pub fn emit(app: &AppHandle, event: &RunEvent) {
    if let Err(e) = app.emit(RUN_EVENT, event) {
        tracing::error!("推送 run-event 失败: {e}");
    }
}
