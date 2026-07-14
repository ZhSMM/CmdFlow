//! 节点抽象
//!
//! 每个节点类型实现 `Node` trait。
//! 节点执行是「黑盒」：接收配置 + 上游数据，输出结果给下游。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::core::events::{NodeStatus, StreamKind};
use crate::core::interpolation::{render, InterpContext};
use crate::error::AppResult;
use crate::storage::db::DbPool;

pub type SharedNode = Arc<dyn Node>;

/// 节点配置 schema (用于 UI 生成表单)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSchema {
    pub type_id: String,
    pub display_name: String,
    pub category: String,
    pub description: String,
    pub config_schema: Value,
}

/// 节点执行上下文
#[derive(Clone)]
pub struct NodeContext {
    pub execution_id: String,
    pub node_id: String,
    pub node_name: String,
    pub app: tauri::AppHandle,
    pub db: Arc<DbPool>,
    pub cancel: CancellationToken,
    /// 上游节点的输出 (node_id -> value)
    pub upstream_outputs: HashMap<String, Value>,
    /// 全局工作流参数 (用户填的)
    pub workflow_params: HashMap<String, String>,
}

/// 节点执行结果
#[derive(Debug, Clone, Default)]
pub struct NodeOutput {
    /// 给下游节点的数据 (可序列化的 JSON 值)
    pub value: Value,
    /// 走哪些输出端口 (空 = 走所有)
    /// 条件节点返回 ["true"] 或 ["false"]
    pub branches: Vec<String>,
    /// 收集到的 stdout / stderr (供 UI 显示 + 持久化)
    pub stdout: String,
    pub stderr: String,
    /// 退出码 (子进程类节点用)
    pub exit_code: Option<i32>,
    /// 状态
    pub status: NodeStatus,
    /// 错误信息
    pub error: Option<String>,
}

impl NodeOutput {
    pub fn success(value: Value) -> Self {
        Self {
            value,
            status: NodeStatus::Success,
            ..Default::default()
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            status: NodeStatus::Failed,
            error: Some(error.into()),
            ..Default::default()
        }
    }
}

/// 节点 trait
#[async_trait]
pub trait Node: Send + Sync {
    /// 节点类型 ID (如 "cmd", "http")
    fn type_id(&self) -> &'static str;

    /// 显示名
    fn display_name(&self) -> &'static str;

    /// 分类 (core / io / control)
    fn category(&self) -> &'static str;

    /// 节点描述
    fn description(&self) -> &'static str {
        ""
    }

    /// 配置 schema (JSON Schema 形式，供 UI 生成表单)
    fn config_schema(&self) -> Value;

    /// 执行节点
    ///
    /// - `config`:   节点配置 (来自数据库的 JSON)
    /// - `ctx`:      执行上下文
    async fn execute(&self, ctx: NodeContext, config: Value) -> AppResult<NodeOutput>;
}

/// 渲染配置中的所有字符串字段 (递归处理 JSON)
///
/// 把 config 中所有 string 类型的值用 {{var}} 插值。
/// 简单实现:遍历所有 Value::String,尝试 render。
pub fn render_config(config: &Value, ctx: &InterpContext) -> AppResult<Value> {
    match config {
        Value::String(s) => render(s, ctx).map(Value::String),
        Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                out.push(render_config(v, ctx)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(obj) => {
            let mut out = serde_json::Map::new();
            for (k, v) in obj {
                out.insert(k.clone(), render_config(v, ctx)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// 把 stream 信息转成 RunEvent (供 executor 推送)
pub fn stream_event(
    app: &tauri::AppHandle,
    execution_id: &str,
    node_id: &str,
    stream: StreamKind,
    content: String,
) {
    use crate::core::events::{emit, RunEvent};
    emit(
        app,
        &RunEvent::NodeLog {
            execution_id: execution_id.to_string(),
            node_id: node_id.to_string(),
            stream,
            content,
            ts: chrono::Utc::now().timestamp_millis(),
        },
    );
}
