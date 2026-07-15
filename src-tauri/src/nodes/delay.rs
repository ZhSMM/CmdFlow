//! 延时节点

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use super::node::{Node, NodeContext, NodeOutput};
use crate::core::events::NodeStatus;

#[derive(Debug, Deserialize)]
struct DelayConfig {
    duration_ms: u64,
}

pub struct DelayNode;

#[async_trait]
impl Node for DelayNode {
    fn type_id(&self) -> &'static str {
        "delay"
    }
    fn display_name(&self) -> &'static str {
        "延时"
    }
    fn category(&self) -> &'static str {
        "control"
    }
    fn description(&self) -> &'static str {
        "等待一段时间"
    }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["duration_ms"],
            "properties": {
                "duration_ms": { "type": "integer", "title": "时长(毫秒)", "minimum": 0 }
            }
        })
    }

    async fn execute(
        &self,
        ctx: NodeContext,
        config: Value,
    ) -> crate::error::AppResult<NodeOutput> {
        let cfg: DelayConfig = serde_json::from_value(config)
            .map_err(|e| crate::error::AppError::invalid(format!("delay config 解析失败: {e}")))?;

        crate::nodes::node::stream_event(
            &ctx.app,
            &ctx.execution_id,
            &ctx.node_id,
            crate::core::events::StreamKind::System,
            format!("⏱ 等待 {} ms\n", cfg.duration_ms),
        );

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(cfg.duration_ms)) => {
                Ok(NodeOutput::success(json!({ "duration_ms": cfg.duration_ms })))
            }
            _ = ctx.cancel.cancelled() => {
                Ok(NodeOutput {
                    status: NodeStatus::Cancelled,
                    error: Some("用户取消".into()),
                    ..Default::default()
                })
            }
        }
    }
}
