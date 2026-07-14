//! 子工作流节点 - 嵌套调用另一个工作流

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use super::node::{Node, NodeContext, NodeOutput};
use crate::core::dag_executor;
use crate::core::events::NodeStatus;

#[derive(Debug, Deserialize)]
struct SubWorkflowConfig {
    workflow_ref: String,
    #[serde(default)]
    pass_params: Vec<String>,
}

pub struct SubWorkflowNode;

#[async_trait]
impl Node for SubWorkflowNode {
    fn type_id(&self) -> &'static str { "subworkflow" }
    fn display_name(&self) -> &'static str { "子工作流" }
    fn category(&self) -> &'static str { "control" }
    fn description(&self) -> &'static str { "嵌套调用另一个工作流" }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["workflow_ref"],
            "properties": {
                "workflow_ref": { "type": "string", "title": "工作流 ID 或名称" },
                "pass_params": {
                    "type": "array",
                    "items": { "type": "string" },
                    "title": "透传参数"
                }
            }
        })
    }

    async fn execute(&self, ctx: NodeContext, config: Value) -> crate::error::AppResult<NodeOutput> {
        let cfg: SubWorkflowConfig = serde_json::from_value(config)
            .map_err(|e| crate::error::AppError::invalid(format!("subworkflow config 解析失败: {e}")))?;

        // 解析 workflow_ref
        let detail = crate::storage::models::WorkflowDetail::load_full(&ctx.db, &cfg.workflow_ref)?
            .or_else(|| {
                // 尝试按名称解析
                None
            })
            .ok_or_else(|| crate::error::AppError::not_found(format!("workflow: {}", cfg.workflow_ref)))?;

        if !detail.workflow.enabled {
            return Ok(NodeOutput::failed("子工作流已禁用".to_string()));
        }

        // 构造子参数: 透传 + workflow_params
        let mut params = ctx.workflow_params.clone();
        for k in &cfg.pass_params {
            if let Some(v) = ctx.workflow_params.get(k) {
                params.insert(k.clone(), v.clone());
            }
        }
        let _ = HashMap::<String, String>::new(); // 抑制未用

        crate::nodes::node::stream_event(&ctx.app, &ctx.execution_id, &ctx.node_id,
            crate::core::events::StreamKind::System,
            format!("🔗 子工作流: {}\n", detail.workflow.name));

        // Phase 4 简化: 同步等待子工作流完成
        // TODO: 真实场景应该异步、复用 execution_id 体系
        let cancel = ctx.cancel.clone();
        let res = dag_executor::execute_workflow(
            ctx.app.clone(),
            ctx.db.clone(),
            uuid::Uuid::new_v4().to_string(),
            detail.workflow.id.clone(),
            detail.workflow.name.clone(),
            detail.nodes.clone(),
            detail.edges.clone(),
            params,
            cancel,
        )
        .await?;

        let value = json!({
            "sub_workflow_id": detail.workflow.id,
            "sub_status": res.status.as_str(),
            "duration_ms": res.duration_ms,
            "node_results": res.node_results.iter().map(|(k, v)| {
                (k.clone(), v.value.clone())
            }).collect::<serde_json::Map<_, _>>()
        });

        Ok(NodeOutput {
            value,
            status: res.status,
            error: if matches!(res.status, NodeStatus::Failed) {
                Some("子工作流失败".into())
            } else { None },
            ..Default::default()
        })
    }
}
