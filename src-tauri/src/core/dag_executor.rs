//! DAG 执行器
//!
//! 负责：
//! 1. 拓扑排序
//! 2. 并行执行同层节点
//! 3. 数据流传递 (上游输出 → 下游输入)
//! 4. 条件分支路由
//! 5. 取消
//! 6. 推送事件

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use serde_json::Value;
use tauri::AppHandle;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::core::events::{emit, NodeStatus, RunEvent};
use crate::error::{AppError, AppResult};
use crate::nodes::node::{NodeContext, NodeOutput};
use crate::nodes::registry;
use crate::storage::db::DbPool;
use crate::storage::models::{WorkflowEdge, WorkflowNode};

/// 整个工作流的执行结果
pub struct WorkflowExecution {
    pub execution_id: String,
    pub status: NodeStatus,
    pub duration_ms: u64,
    pub node_results: HashMap<String, NodeOutput>,
}

/// 边条件
#[derive(Debug, Clone, Default)]
struct EdgeCondition {
    kind: String,          // on_success | on_failure | always | expr
    branches: Vec<String>, // 期望的 source 输出端口
}

impl EdgeCondition {
    fn from_value(v: &Value) -> Self {
        let mut ec = EdgeCondition::default();
        if let Some(obj) = v.as_object() {
            ec.kind = obj
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("on_success")
                .to_string();
            if let Some(arr) = obj.get("branches").and_then(|v| v.as_array()) {
                ec.branches = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
            }
        }
        ec
    }

    fn should_traverse(&self, source_status: NodeStatus, _source_branches: &[String]) -> bool {
        match self.kind.as_str() {
            "always" => true,
            "on_success" => source_status == NodeStatus::Success,
            "on_failure" => matches!(source_status, NodeStatus::Failed | NodeStatus::Timeout),
            "cancelled" => source_status == NodeStatus::Cancelled,
            "expr" => true, // 简化: expr 总是通过
            _ => true,
        }
    }

    fn branches_match(&self, source_branches: &[String]) -> bool {
        // 没声明期望分支 → 任何分支都通过
        if self.branches.is_empty() {
            return true;
        }
        // source_branches 为空 (非条件节点) → 默认走所有
        if source_branches.is_empty() {
            return true;
        }
        // 有重叠即可
        self.branches.iter().any(|b| source_branches.contains(b))
    }
}

/// 执行一个工作流
#[allow(clippy::too_many_arguments)]
pub async fn execute_workflow(
    app: AppHandle,
    db: Arc<DbPool>,
    execution_id: String,
    workflow_id: String,
    workflow_name: String,
    nodes: Vec<WorkflowNode>,
    edges: Vec<WorkflowEdge>,
    workflow_params: HashMap<String, String>,
    cancel: CancellationToken,
) -> AppResult<WorkflowExecution> {
    let start = std::time::Instant::now();

    // 1. 拓扑排序
    let _topo = topological_sort(&nodes, &edges)?;
    let parallel_limit = 4; // Phase 2 固定 4 并发

    // 2. 推送执行开始
    emit(
        &app,
        &RunEvent::ExecutionStarted {
            execution_id: execution_id.clone(),
            command_id: workflow_id.clone(),
            command_name: workflow_name.clone(),
            started_at: chrono::Utc::now().timestamp(),
        },
    );

    // 3. 数据流存储
    let node_outputs: Arc<tokio::sync::RwLock<HashMap<String, Value>>> =
        Arc::new(tokio::sync::RwLock::new(HashMap::new()));
    let node_statuses: Arc<tokio::sync::RwLock<HashMap<String, NodeOutput>>> =
        Arc::new(tokio::sync::RwLock::new(HashMap::new()));

    // 4. 边索引
    let outgoing: HashMap<String, Vec<(String, EdgeCondition)>> = {
        let mut m: HashMap<String, Vec<(String, EdgeCondition)>> = HashMap::new();
        for e in &edges {
            m.entry(e.source_node.clone()).or_default().push((
                e.target_node.clone(),
                EdgeCondition::from_value(&e.condition.clone().unwrap_or(Value::Null)),
            ));
        }
        m
    };

    // 5. 入度 + 邻接表 (Kahn)
    let mut indeg: HashMap<String, usize> = HashMap::new();
    for n in &nodes {
        indeg.insert(n.id.clone(), 0);
    }
    for e in &edges {
        *indeg.entry(e.target_node.clone()).or_insert(0) += 1;
    }

    let mut queue: VecDeque<String> = indeg
        .iter()
        .filter_map(|(id, d)| if *d == 0 { Some(id.clone()) } else { None })
        .collect();

    let sem = Arc::new(Semaphore::new(parallel_limit));
    let _total = nodes.len();
    let mut had_failure = false;

    while !queue.is_empty() {
        // 收集当前所有可执行节点, 并发启动
        let mut handles = Vec::new();
        let batch: Vec<String> = queue.drain(..).collect();

        for node_id in batch {
            let permit = sem.clone().acquire_owned().await.unwrap();

            // 找到对应的 node 定义
            let node_def = match nodes.iter().find(|n| n.id == node_id) {
                Some(n) => n.clone(),
                None => continue,
            };

            let app2 = app.clone();
            let db2 = db.clone();
            let cancel2 = cancel.clone();
            let exec_id2 = execution_id.clone();
            let wf_id2 = workflow_id.clone();
            let node_outputs2 = node_outputs.clone();
            let node_statuses2 = node_statuses.clone();
            let params2 = workflow_params.clone();

            let handle = tokio::spawn(async move {
                let _permit = permit;
                run_one_node(
                    app2,
                    db2,
                    exec_id2,
                    wf_id2,
                    node_def,
                    params2,
                    node_outputs2,
                    node_statuses2,
                    cancel2,
                )
                .await
            });

            handles.push((node_id, handle));
        }

        // 收集结果, 更新 indeg + queue
        for (node_id, h) in handles {
            let res = h
                .await
                .map_err(|e| AppError::other(format!("join 失败: {e}")))?;
            match res {
                Ok(output) => {
                    // 存数据
                    let status = output.status;
                    let branches = output.branches.clone();

                    {
                        let mut s = node_statuses.write().await;
                        s.insert(node_id.clone(), output.clone());
                    }
                    {
                        let mut o = node_outputs.write().await;
                        o.insert(node_id.clone(), output.value.clone());
                    }

                    // 触发下游
                    if let Some(targets) = outgoing.get(&node_id) {
                        for (target, cond) in targets {
                            if !cond.should_traverse(status, &branches) {
                                continue;
                            }
                            if !cond.branches_match(&branches) {
                                continue;
                            }
                            if let Some(d) = indeg.get_mut(target) {
                                *d = d.saturating_sub(1);
                                if *d == 0 {
                                    queue.push_back(target.clone());
                                }
                            }
                        }
                    }

                    if matches!(status, NodeStatus::Failed | NodeStatus::Timeout) {
                        had_failure = true;
                    }
                }
                Err(e) => {
                    had_failure = true;
                    let output = NodeOutput::failed(e.to_string());
                    {
                        let mut s = node_statuses.write().await;
                        s.insert(node_id.clone(), output);
                    }
                    // 即使失败也尝试把 downstream 标完成 (除非条件需要 success)
                    if let Some(targets) = outgoing.get(&node_id) {
                        for (target, cond) in targets {
                            // on_failure / always 仍然往下
                            if matches!(cond.kind.as_str(), "on_failure" | "always") {
                                if let Some(d) = indeg.get_mut(target) {
                                    *d = d.saturating_sub(1);
                                    if *d == 0 {
                                        queue.push_back(target.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    let final_status = if cancel.is_cancelled() {
        NodeStatus::Cancelled
    } else if had_failure {
        NodeStatus::Failed
    } else {
        NodeStatus::Success
    };

    // 推送完成
    let node_results = node_statuses.read().await.clone();
    emit(
        &app,
        &RunEvent::ExecutionFinished {
            execution_id: execution_id.clone(),
            status: final_status,
            duration_ms,
            error: if had_failure {
                Some("部分节点失败".into())
            } else {
                None
            },
        },
    );

    Ok(WorkflowExecution {
        execution_id,
        status: final_status,
        duration_ms,
        node_results,
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_one_node(
    app: AppHandle,
    db: Arc<DbPool>,
    execution_id: String,
    _workflow_id: String,
    node: WorkflowNode,
    workflow_params: HashMap<String, String>,
    node_outputs: Arc<tokio::sync::RwLock<HashMap<String, Value>>>,
    _node_statuses: Arc<tokio::sync::RwLock<HashMap<String, NodeOutput>>>,
    cancel: CancellationToken,
) -> AppResult<NodeOutput> {
    let started_at = chrono::Utc::now().timestamp();
    emit(
        &app,
        &RunEvent::NodeStarted {
            execution_id: execution_id.clone(),
            node_id: node.id.clone(),
            node_name: format!("{} ({})", node.node_type, &node.id[..8.min(node.id.len())]),
            started_at,
        },
    );

    // 找节点实现
    let node_impl = registry::registry()
        .get(&node.node_type)
        .ok_or_else(|| AppError::invalid(format!("未知节点类型: {}", node.node_type)))?;

    // 构造 ctx
    let upstream = node_outputs.read().await.clone();
    let ctx = NodeContext {
        execution_id: execution_id.clone(),
        node_id: node.id.clone(),
        node_name: node.node_type.clone(),
        app: app.clone(),
        db: db.clone(),
        cancel: cancel.clone(),
        upstream_outputs: upstream,
        workflow_params: workflow_params.clone(),
    };

    // 执行
    let start = std::time::Instant::now();
    let result = node_impl.execute(ctx, node.config.clone()).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    let output = match result {
        Ok(o) => o,
        Err(e) => NodeOutput::failed(e.to_string()),
    };

    // 持久化 node_run
    let _ = persist_node_run(&db, &execution_id, &node, &output, started_at, duration_ms).await;

    // 推结束事件
    emit(
        &app,
        &RunEvent::NodeFinished {
            execution_id: execution_id.clone(),
            node_id: node.id.clone(),
            exit_code: output.exit_code,
            status: output.status,
            duration_ms,
        },
    );

    if output.status == NodeStatus::Success {
        Ok(output)
    } else {
        Err(AppError::other(output.error.clone().unwrap_or_else(|| {
            format!("节点失败: {:?}", output.status)
        })))
    }
}

async fn persist_node_run(
    db: &DbPool,
    execution_id: &str,
    node: &WorkflowNode,
    output: &NodeOutput,
    started_at: i64,
    duration_ms: u64,
) -> AppResult<()> {
    let conn = db.get()?;
    let node_run_id = uuid::Uuid::new_v4().to_string();
    let value_str = serde_json::to_string(&output.value).ok();
    let finished_at = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO node_runs (id, execution_id, node_id, status, started_at, finished_at, duration_ms, exit_code, stdout, stderr, output_data, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            node_run_id,
            execution_id,
            node.id,
            output.status.as_str(),
            started_at,
            finished_at,
            duration_ms as i64,
            output.exit_code,
            output.stdout,
            output.stderr,
            value_str,
            output.error,
        ],
    )?;

    Ok(())
}

fn topological_sort(nodes: &[WorkflowNode], edges: &[WorkflowEdge]) -> AppResult<Vec<String>> {
    let node_ids: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();

    let mut indeg: HashMap<String, usize> = HashMap::new();
    for n in nodes {
        indeg.insert(n.id.clone(), 0);
    }
    for e in edges {
        if !node_ids.contains(&e.source_node) || !node_ids.contains(&e.target_node) {
            continue;
        }
        *indeg.entry(e.target_node.clone()).or_insert(0) += 1;
    }

    let mut queue: VecDeque<String> = indeg
        .iter()
        .filter_map(|(id, d)| if *d == 0 { Some(id.clone()) } else { None })
        .collect();

    let mut topo = Vec::new();
    while let Some(u) = queue.pop_front() {
        topo.push(u.clone());
        for e in edges.iter().filter(|e| e.source_node == u) {
            if let Some(d) = indeg.get_mut(&e.target_node) {
                *d -= 1;
                if *d == 0 {
                    queue.push_back(e.target_node.clone());
                }
            }
        }
    }

    if topo.len() != nodes.len() {
        return Err(AppError::dag("工作流存在环"));
    }
    Ok(topo)
}
