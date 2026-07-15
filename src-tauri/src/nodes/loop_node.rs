//! 循环节点 - 简化实现
//!
//! Phase 2 简化:对数组/数字范围迭代。
//! 完整的「子图 body」留到 Phase 4 实现 (SubWorkflow 模式)。
//! Phase 2 这里只把循环体走完一遍,body 暂时用 config.body.code (类似 script)

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

use super::node::{Node, NodeContext, NodeOutput};
use crate::core::interpolation::InterpContext;

#[derive(Debug, Deserialize)]
struct LoopConfig {
    /// 集合表达式,如 "{{nodeId.items}}" 或 "1..10"
    over: String,
    #[serde(default = "default_iter")]
    iterator_var: String,
    #[serde(default = "default_max")]
    max_iterations: usize,
}

fn default_iter() -> String {
    "item".to_string()
}
fn default_max() -> usize {
    1000
}

pub struct LoopNode;

#[async_trait]
impl Node for LoopNode {
    fn type_id(&self) -> &'static str {
        "loop"
    }
    fn display_name(&self) -> &'static str {
        "循环"
    }
    fn category(&self) -> &'static str {
        "control"
    }
    fn description(&self) -> &'static str {
        "对集合迭代 (Phase 2 简化版:仅收集 items)"
    }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["over"],
            "properties": {
                "over": { "type": "string", "title": "集合表达式" },
                "iterator_var": { "type": "string", "title": "迭代变量名" },
                "max_iterations": { "type": "integer", "title": "最大迭代数" }
            }
        })
    }

    async fn execute(
        &self,
        _ctx: NodeContext,
        config: Value,
    ) -> crate::error::AppResult<NodeOutput> {
        let cfg: LoopConfig = serde_json::from_value(config)
            .map_err(|e| crate::error::AppError::invalid(format!("loop config 解析失败: {e}")))?;

        // 解析 over
        let items = resolve_collection(&cfg.over, &_ctx).unwrap_or_default();
        let count = items.len().min(cfg.max_iterations);

        crate::nodes::node::stream_event(
            &_ctx.app,
            &_ctx.execution_id,
            &_ctx.node_id,
            crate::core::events::StreamKind::System,
            format!("🔁 循环 {} 次 (max {})\n", count, cfg.max_iterations),
        );

        // Phase 2 简化: 只把 items 返回,具体的 body 执行通过下游节点读取
        // 完整版 (子图 body) 在 Phase 4 通过 SubWorkflow + 多实例实现
        let mut out_items = Vec::new();
        for (i, item) in items.iter().take(count).enumerate() {
            crate::nodes::node::stream_event(
                &_ctx.app,
                &_ctx.execution_id,
                &_ctx.node_id,
                crate::core::events::StreamKind::Stdout,
                format!("[{}/{}] {} = {}\n", i + 1, count, cfg.iterator_var, item),
            );
            out_items.push(item.clone());
        }

        Ok(NodeOutput::success(json!({
            "items": out_items,
            "count": count,
        })))
    }
}

fn resolve_collection(expr: &str, ctx: &NodeContext) -> Option<Vec<Value>> {
    let mut interp = InterpContext::new();
    for (k, v) in &ctx.workflow_params {
        interp.params.insert(k.clone(), v.clone());
    }
    for (k, v) in &ctx.upstream_outputs {
        interp.node_outputs.insert(k.clone(), v.clone());
    }
    let rendered = crate::core::interpolation::render(expr, &interp).ok()?;

    // 范围 "1..10"
    if let Some(idx) = rendered.find("..") {
        let a: i64 = rendered[..idx].trim().parse().ok()?;
        let b: i64 = rendered[idx + 2..].trim().parse().ok()?;
        return Some((a.min(b)..=a.max(b)).map(|n| json!(n)).collect());
    }

    // 数字字面量
    if let Ok(n) = rendered.parse::<i64>() {
        return Some(vec![json!(n)]);
    }

    // JSON 数组
    if let Ok(v) = serde_json::from_str::<Value>(&rendered) {
        if let Value::Array(arr) = v {
            return Some(arr);
        }
    }

    // 单值
    Some(vec![Value::String(rendered)])
}
