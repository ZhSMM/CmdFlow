//! 工作流 IPC

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, State};
use tokio::task;

use crate::core::dag;
use crate::core::dag_executor;
use crate::error::{AppError, AppResult};
use crate::nodes::registry as node_registry;
use crate::state::AppState;
use crate::storage::models::{Workflow, WorkflowDetail, WorkflowEdge, WorkflowNode};

#[tauri::command]
pub fn list_workflows(state: State<'_, AppState>) -> AppResult<Vec<Workflow>> {
    let conn = state.db.get()?;
    let mut stmt = conn.prepare(
        "SELECT id, name, description, enabled, trigger_type, created_at, updated_at
         FROM workflows ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Workflow {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            enabled: r.get(3)?,
            trigger_type: r.get(4)?,
            created_at: r.get(5)?,
            updated_at: r.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[tauri::command]
pub fn get_workflow(state: State<'_, AppState>, id: String) -> AppResult<WorkflowDetail> {
    let conn = state.db.get()?;

    let workflow: Workflow = conn
        .query_row(
            "SELECT id, name, description, enabled, trigger_type, created_at, updated_at
         FROM workflows WHERE id = ?1",
            [&id],
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
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::not_found(format!("workflow:{id}")),
            other => AppError::Database(other),
        })?;

    let mut nstmt = conn.prepare(
        "SELECT id, workflow_id, type, command_id, config, position_x, position_y, sort_order
         FROM nodes WHERE workflow_id = ?1 ORDER BY sort_order",
    )?;
    let nodes: Vec<WorkflowNode> = nstmt
        .query_map([&id], |r| {
            Ok(WorkflowNode {
                id: r.get(0)?,
                workflow_id: r.get(1)?,
                node_type: r.get::<_, String>(2)?,
                command_id: r.get(3)?,
                config: r
                    .get::<_, String>(4)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::Value::Null),
                position_x: r.get(5)?,
                position_y: r.get(6)?,
                sort_order: r.get(7)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    let mut estmt = conn.prepare(
        "SELECT id, workflow_id, source_node, target_node, source_port, target_port, condition
         FROM edges WHERE workflow_id = ?1",
    )?;
    let edges: Vec<WorkflowEdge> = estmt
        .query_map([&id], |r| {
            Ok(WorkflowEdge {
                id: r.get(0)?,
                workflow_id: r.get(1)?,
                source_node: r.get(2)?,
                target_node: r.get(3)?,
                source_port: r.get(4)?,
                target_port: r.get(5)?,
                condition: r
                    .get::<_, Option<String>>(6)?
                    .and_then(|s| serde_json::from_str(&s).ok()),
            })
        })?
        .filter_map(Result::ok)
        .collect();

    Ok(WorkflowDetail {
        workflow,
        nodes,
        edges,
    })
}

#[derive(serde::Deserialize)]
pub struct CreateWorkflowInput {
    pub name: String,
    pub description: Option<String>,
    pub trigger_type: Option<String>,
}

#[tauri::command]
pub fn create_workflow(
    state: State<'_, AppState>,
    input: CreateWorkflowInput,
) -> AppResult<String> {
    let conn = state.db.get()?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO workflows (id, name, description, enabled, trigger_type, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6)",
        rusqlite::params![
            &id,
            &input.name,
            &input.description,
            input.trigger_type.as_deref().unwrap_or("manual"),
            now,
            now,
        ],
    )?;
    Ok(id)
}

#[derive(serde::Deserialize)]
pub struct UpdateWorkflowInput {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub trigger_type: Option<String>,
    pub nodes: Option<Vec<WorkflowNode>>,
    pub edges: Option<Vec<WorkflowEdge>>,
}

#[tauri::command]
pub fn update_workflow(state: State<'_, AppState>, input: UpdateWorkflowInput) -> AppResult<()> {
    let conn = state.db.get()?;
    let now = chrono::Utc::now().timestamp();
    let tx = conn.unchecked_transaction()?;

    // 顶层字段
    let mut updates: Vec<&str> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(n) = &input.name {
        updates.push("name = ?");
        args.push(Box::new(n.clone()));
    }
    if let Some(d) = &input.description {
        updates.push("description = ?");
        args.push(Box::new(d.clone()));
    }
    if let Some(e) = &input.enabled {
        updates.push("enabled = ?");
        args.push(Box::new(*e as i32));
    }
    if let Some(t) = &input.trigger_type {
        updates.push("trigger_type = ?");
        args.push(Box::new(t.clone()));
    }
    if !updates.is_empty() {
        updates.push("updated_at = ?");
        args.push(Box::new(now));
        args.push(Box::new(input.id.clone()));
        let sql = format!("UPDATE workflows SET {} WHERE id = ?", updates.join(", "));
        let arg_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|a| a.as_ref()).collect();
        tx.execute(&sql, &*arg_refs)?;
    }

    // nodes / edges: 全删全插
    if let Some(nodes) = &input.nodes {
        tx.execute("DELETE FROM nodes WHERE workflow_id = ?1", [&input.id])?;
        for (idx, n) in nodes.iter().enumerate() {
            tx.execute(
                "INSERT INTO nodes (id, workflow_id, type, command_id, config, position_x, position_y, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    &n.id,
                    &input.id,
                    &n.node_type,
                    &n.command_id,
                    serde_json::to_string(&n.config)?,
                    n.position_x,
                    n.position_y,
                    idx as i32,
                ],
            )?;
        }
    }
    if let Some(edges) = &input.edges {
        tx.execute("DELETE FROM edges WHERE workflow_id = ?1", [&input.id])?;
        for e in edges {
            tx.execute(
                "INSERT INTO edges (id, workflow_id, source_node, target_node, source_port, target_port, condition)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    &e.id,
                    &input.id,
                    &e.source_node,
                    &e.target_node,
                    &e.source_port,
                    &e.target_port,
                    e.condition.as_ref().map(serde_json::to_string).transpose()?,
                ],
            )?;
        }
    }

    tx.commit()?;
    Ok(())
}

#[tauri::command]
pub fn delete_workflow(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let conn = state.db.get()?;
    conn.execute("DELETE FROM workflows WHERE id = ?1", [&id])?;
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct ValidateDagInput {
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
}

#[derive(serde::Serialize)]
pub struct DagValidation {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub topo_order: Option<Vec<String>>,
    pub parallel_layers: Option<Vec<Vec<String>>>,
}

#[tauri::command]
pub fn validate_dag(input: ValidateDagInput) -> AppResult<DagValidation> {
    dag::validate(&input.nodes, &input.edges)
        .map(|result| DagValidation {
            ok: result.errors.is_empty(),
            errors: result.errors,
            warnings: result.warnings,
            topo_order: Some(result.topo_order),
            parallel_layers: Some(result.parallel_layers),
        })
        .ok_or_else(|| AppError::dag("DAG 验证失败"))
}

// ==================== 工作流执行 ====================

#[derive(serde::Deserialize)]
pub struct RunWorkflowInput {
    pub workflow_id: String,
    /// 工作流级别的参数 (会传入到所有节点的 {{param}} 插值)
    #[serde(default)]
    pub params: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct RunWorkflowResponse {
    pub execution_id: String,
}

#[tauri::command]
pub async fn run_workflow(
    app: AppHandle,
    state: State<'_, AppState>,
    input: RunWorkflowInput,
) -> AppResult<RunWorkflowResponse> {
    // 1. 加载 workflow 详情
    let detail = get_workflow_inner(&state, &input.workflow_id).await?;

    if !detail.workflow.enabled {
        return Err(AppError::invalid("工作流已禁用"));
    }
    if detail.nodes.is_empty() {
        return Err(AppError::invalid("工作流没有节点"));
    }

    // 2. 注册 cancel
    let execution_id = uuid::Uuid::new_v4().to_string();
    let registry = state.execution_registry.clone();
    let cancel = registry.register(execution_id.clone()).await;

    // 3. 写 executions 表
    {
        let conn = state.db.get()?;
        conn.execute(
            "INSERT INTO executions (id, workflow_id, trigger, status, started_at, input_params)
             VALUES (?1, ?2, 'manual', 'running', ?3, ?4)",
            rusqlite::params![
                execution_id,
                input.workflow_id,
                chrono::Utc::now().timestamp(),
                serde_json::to_string(&input.params).ok(),
            ],
        )?;
    }

    // 4. 异步执行
    let app2 = app.clone();
    let db2 = state.db.clone();
    let nodes = detail.nodes.clone();
    let edges = detail.edges.clone();
    let wf_id = input.workflow_id.clone();
    let wf_name = detail.workflow.name.clone();
    let exec_id2 = execution_id.clone();
    let params = input.params.clone();

    task::spawn(async move {
        let res = dag_executor::execute_workflow(
            app2.clone(),
            db2.clone(),
            exec_id2.clone(),
            wf_id.clone(),
            wf_name,
            nodes,
            edges,
            params,
            cancel,
        )
        .await;

        // 更新 executions 表
        let conn = match db2.get() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("get conn 失败: {e}");
                return;
            }
        };
        match res {
            Ok(r) => {
                let _ = conn.execute(
                    "UPDATE executions SET status = ?1, finished_at = ?2, duration_ms = ?3, error = NULL WHERE id = ?4",
                    rusqlite::params![
                        r.status.as_str(),
                        chrono::Utc::now().timestamp(),
                        r.duration_ms as i64,
                        r.execution_id,
                    ],
                );
            }
            Err(e) => {
                let _ = conn.execute(
                    "UPDATE executions SET status = 'failed', finished_at = ?1, error = ?2 WHERE id = ?3",
                    rusqlite::params![
                        chrono::Utc::now().timestamp(),
                        e.to_string(),
                        exec_id2,
                    ],
                );
            }
        }
    });

    Ok(RunWorkflowResponse { execution_id })
}

#[tauri::command]
pub async fn cancel_workflow_execution(
    state: State<'_, AppState>,
    execution_id: String,
) -> AppResult<bool> {
    Ok(state.execution_registry.cancel(&execution_id).await)
}

#[tauri::command]
pub fn list_node_types() -> AppResult<Vec<serde_json::Value>> {
    let schemas = node_registry::list_node_types();
    Ok(schemas
        .into_iter()
        .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        .collect())
}

#[tauri::command]
pub fn get_node_schema(type_id: String) -> AppResult<serde_json::Value> {
    node_registry::get_node_schema(&type_id)
        .ok_or_else(|| AppError::not_found(format!("node type: {type_id}")))
}

// ==================== 内部 helper ====================

async fn get_workflow_inner(state: &State<'_, AppState>, id: &str) -> AppResult<WorkflowDetail> {
    let conn = state.db.get()?;

    let workflow: Workflow = conn
        .query_row(
            "SELECT id, name, description, enabled, trigger_type, created_at, updated_at
         FROM workflows WHERE id = ?1",
            [id],
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
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::not_found(format!("workflow:{id}")),
            other => AppError::Database(other),
        })?;

    let mut nstmt = conn.prepare(
        "SELECT id, workflow_id, type, command_id, config, position_x, position_y, sort_order
         FROM nodes WHERE workflow_id = ?1 ORDER BY sort_order",
    )?;
    let nodes: Vec<WorkflowNode> = nstmt
        .query_map([id], |r| {
            Ok(WorkflowNode {
                id: r.get(0)?,
                workflow_id: r.get(1)?,
                node_type: r.get::<_, String>(2)?,
                command_id: r.get(3)?,
                config: r
                    .get::<_, String>(4)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(serde_json::Value::Null),
                position_x: r.get(5)?,
                position_y: r.get(6)?,
                sort_order: r.get(7)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    let mut estmt = conn.prepare(
        "SELECT id, workflow_id, source_node, target_node, source_port, target_port, condition
         FROM edges WHERE workflow_id = ?1",
    )?;
    let edges: Vec<WorkflowEdge> = estmt
        .query_map([id], |r| {
            Ok(WorkflowEdge {
                id: r.get(0)?,
                workflow_id: r.get(1)?,
                source_node: r.get(2)?,
                target_node: r.get(3)?,
                source_port: r.get(4)?,
                target_port: r.get(5)?,
                condition: r
                    .get::<_, Option<String>>(6)
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str(&s).ok()),
            })
        })?
        .filter_map(Result::ok)
        .collect();

    Ok(WorkflowDetail {
        workflow,
        nodes,
        edges,
    })
}
