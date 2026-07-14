//! 节点注册表

use std::sync::{Arc, OnceLock, RwLock};

use serde_json::Value;

use super::ai::AiNode;
use super::cmd::CmdNode;
use super::condition::ConditionNode;
use super::delay::DelayNode;
use super::file::FileNode;
use super::http::HttpNode;
use super::loop_node::LoopNode;
use super::node::{Node, NodeSchema, SharedNode};
use super::script::ScriptNode;
use super::subworkflow::SubWorkflowNode;

pub struct NodeRegistry {
    nodes: RwLock<Vec<SharedNode>>,
}

impl NodeRegistry {
    fn new() -> Self {
        Self {
            nodes: RwLock::new(Vec::new()),
        }
    }

    fn push(&self, node: SharedNode) {
        self.nodes.write().unwrap().push(node);
    }

    pub fn get(&self, type_id: &str) -> Option<SharedNode> {
        self.nodes
            .read()
            .unwrap()
            .iter()
            .find(|n| n.type_id() == type_id)
            .cloned()
    }

    pub fn list(&self) -> Vec<NodeSchema> {
        self.nodes
            .read()
            .unwrap()
            .iter()
            .map(|n| NodeSchema {
                type_id: n.type_id().to_string(),
                display_name: n.display_name().to_string(),
                category: n.category().to_string(),
                description: n.description().to_string(),
                config_schema: n.config_schema(),
            })
            .collect()
    }
}

static REGISTRY: OnceLock<NodeRegistry> = OnceLock::new();

/// 获取全局注册表（首次调用时初始化）
pub fn registry() -> &'static NodeRegistry {
    REGISTRY.get_or_init(|| {
        let r = NodeRegistry::new();
        r.push(Arc::new(CmdNode) as SharedNode);
        r.push(Arc::new(ScriptNode) as SharedNode);
        r.push(Arc::new(HttpNode) as SharedNode);
        r.push(Arc::new(FileNode) as SharedNode);
        r.push(Arc::new(DelayNode) as SharedNode);
        r.push(Arc::new(ConditionNode) as SharedNode);
        r.push(Arc::new(LoopNode) as SharedNode);
        r.push(Arc::new(AiNode) as SharedNode);
        r.push(Arc::new(SubWorkflowNode) as SharedNode);
        r
    })
}

/// 列出所有节点类型 (同步)
pub fn list_node_types() -> Vec<NodeSchema> {
    registry().list()
}

/// 获取特定节点类型的 schema
pub fn get_node_schema(type_id: &str) -> Option<Value> {
    registry().get(type_id).map(|n| n.config_schema())
}
