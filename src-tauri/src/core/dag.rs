//! DAG 解析、校验、拓扑排序
//!
//! 提供环检测、拓扑排序、并行层划分。

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::AppResult;
use crate::storage::models::{WorkflowEdge, WorkflowNode};

pub struct DagValidationResult {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub topo_order: Vec<String>,
    pub parallel_layers: Vec<Vec<String>>,
}

/// 校验 DAG，返回错误/警告/拓扑序/并行层。
/// 任何输入（即使有错误）也尽量返回部分结果，便于 UI 高亮问题。
pub fn validate(nodes: &[WorkflowNode], edges: &[WorkflowEdge]) -> Option<DagValidationResult> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. 节点 ID 唯一性
    let mut seen = HashSet::new();
    for n in nodes {
        if !seen.insert(n.id.clone()) {
            errors.push(format!("重复的节点 ID: {}", n.id));
        }
    }

    // 2. 边引用的节点必须存在
    let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    for e in edges {
        if !node_ids.contains(e.source_node.as_str()) {
            errors.push(format!(
                "边 {} 引用了不存在的源节点: {}",
                e.id, e.source_node
            ));
        }
        if !node_ids.contains(e.target_node.as_str()) {
            errors.push(format!(
                "边 {} 引用了不存在的目标节点: {}",
                e.id, e.target_node
            ));
        }
    }

    // 3. 环检测 + 拓扑排序 (Kahn 算法)
    let mut indeg: HashMap<&str, usize> = HashMap::new();
    for n in nodes {
        indeg.insert(n.id.as_str(), 0);
    }
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in edges {
        *indeg.entry(e.target_node.as_str()).or_insert(0) += 1;
        adj.entry(e.source_node.as_str())
            .or_default()
            .push(e.target_node.as_str());
    }

    let mut queue: VecDeque<&str> = indeg
        .iter()
        .filter_map(|(id, d)| if *d == 0 { Some(*id) } else { None })
        .collect();

    let mut topo = Vec::new();
    while let Some(u) = queue.pop_front() {
        topo.push(u.to_string());
        if let Some(next) = adj.get(u) {
            for v in next {
                if let Some(d) = indeg.get_mut(v) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(v);
                    }
                }
            }
        }
    }

    if topo.len() != nodes.len() {
        // 找出环里的节点
        let in_topo: HashSet<&str> = topo.iter().map(|s| s.as_str()).collect();
        let cycle_nodes: Vec<String> = nodes
            .iter()
            .filter(|n| !in_topo.contains(n.id.as_str()))
            .map(|n| n.id.clone())
            .collect();
        errors.push(format!("检测到环，节点: {}", cycle_nodes.join(", ")));
    }

    // 4. 孤立节点警告
    let connected: HashSet<&str> = edges
        .iter()
        .flat_map(|e| [e.source_node.as_str(), e.target_node.as_str()])
        .collect();
    for n in nodes {
        if !connected.contains(n.id.as_str()) && nodes.len() > 1 {
            warnings.push(format!("节点 {} 未连接到任何边", n.id));
        }
    }

    // 5. 并行层划分
    let parallel_layers = compute_parallel_layers(nodes, edges);

    Some(DagValidationResult {
        errors,
        warnings,
        topo_order: topo,
        parallel_layers,
    })
}

/// 把节点按「依赖深度」分层，每层内的节点可并行执行。
fn compute_parallel_layers(nodes: &[WorkflowNode], edges: &[WorkflowEdge]) -> Vec<Vec<String>> {
    let mut depth: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in nodes {
        depth.insert(n.id.as_str(), 0);
    }
    for e in edges {
        adj.entry(e.source_node.as_str())
            .or_default()
            .push(e.target_node.as_str());
    }

    // 简单迭代：直到所有深度稳定
    let mut changed = true;
    let mut iter = 0;
    while changed && iter < 1000 {
        changed = false;
        iter += 1;
        for e in edges {
            let s_depth = *depth.get(e.source_node.as_str()).unwrap_or(&0);
            let t_entry = depth.entry(e.target_node.as_str()).or_insert(0);
            if s_depth + 1 > *t_entry {
                *t_entry = s_depth + 1;
                changed = true;
            }
        }
    }

    // 按 depth 分组
    let max_depth = depth.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_depth + 1];
    for (id, d) in &depth {
        layers[*d].push(id.to_string());
    }
    layers
}

/// 解析阶段给的入口：仅做校验
pub fn validate_or_error(
    nodes: &[WorkflowNode],
    edges: &[WorkflowEdge],
) -> AppResult<DagValidationResult> {
    validate(nodes, edges)
        .filter(|r| r.errors.is_empty())
        .ok_or_else(|| crate::error::AppError::dag("DAG 校验失败"))
}
