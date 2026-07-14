//! 工作流 YAML 导入导出 (Phase 8)

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// YAML 顶层结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowYaml {
    pub version: u32,
    pub kind: String, // 必须是 "workflow"
    pub metadata: WorkflowYamlMetadata,
    pub nodes: Vec<WorkflowYamlNode>,
    pub edges: Vec<WorkflowYamlEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowYamlMetadata {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowYamlNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_ref: Option<String>,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub position: Option<Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowYamlEdge {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn export_workflow(
    state: State<'_, AppState>,
    id: String,
) -> AppResult<String> {
    let detail = crate::storage::models::WorkflowDetail::load_full(&state.db, &id)?
        .ok_or_else(|| AppError::not_found(format!("workflow: {id}")))?;

    let yaml_doc = WorkflowYaml {
        version: 1,
        kind: "workflow".to_string(),
        metadata: WorkflowYamlMetadata {
            name: detail.workflow.name.clone(),
            description: detail.workflow.description.clone(),
            trigger: Some(detail.workflow.trigger_type.clone()),
        },
        nodes: detail.nodes.iter().map(|n| WorkflowYamlNode {
            id: n.id.clone(),
            node_type: n.node_type.clone(),
            command_ref: n.command_id.clone(),
            config: n.config.clone(),
            position: Some(Position { x: n.position_x, y: n.position_y }),
        }).collect(),
        edges: detail.edges.iter().map(|e| WorkflowYamlEdge {
            from: e.source_node.clone(),
            to: e.target_node.clone(),
            label: None,
            condition: e.condition.clone(),
        }).collect(),
    };

    let yaml = serde_yaml::to_string(&yaml_doc)
        .map_err(|e| AppError::other(format!("YAML 序列化失败: {e}")))?;
    Ok(yaml)
}

#[derive(Serialize)]
pub struct ImportResult {
    pub workflow_id: String,
    pub name: String,
}

#[tauri::command]
pub async fn import_workflow(
    state: State<'_, AppState>,
    yaml: String,
) -> AppResult<ImportResult> {
    let doc: WorkflowYaml = serde_yaml::from_str(&yaml)
        .map_err(|e| AppError::other(format!("YAML 解析失败: {e}")))?;
    if doc.kind != "workflow" {
        return Err(AppError::other(format!("不支持的 kind: {} (期望 workflow)", doc.kind)));
    }

    // 1. 创建工作流
    let workflow_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let conn = state.db.get()?;
    conn.execute(
        "INSERT INTO workflows (id, name, description, enabled, trigger_type, created_at, updated_at)
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5)",
        rusqlite::params![
            workflow_id,
            doc.metadata.name,
            doc.metadata.description,
            doc.metadata.trigger.unwrap_or_else(|| "manual".to_string()),
            now,
        ],
    )?;
    drop(conn);

    // 2. 插入 nodes
    let conn = state.db.get()?;
    let tx = conn.unchecked_transaction()?;
    for (i, n) in doc.nodes.iter().enumerate() {
        // 重新生成 ID 避免冲突
        let new_id = if n.id.starts_with("n_") {
            format!("n_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
        } else {
            n.id.clone()
        };
        let (px, py) = n.position.as_ref().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
        tx.execute(
            "INSERT INTO nodes (id, workflow_id, type, command_id, config, position_x, position_y, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                new_id,
                workflow_id,
                n.node_type,
                n.command_ref,
                serde_json::to_string(&n.config)?,
                px,
                py,
                i as i32,
            ],
        )?;
    }
    // 3. 插入 edges
    for e in &doc.edges {
        let new_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO edges (id, workflow_id, source_node, target_node, source_port, target_port, condition)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)",
            rusqlite::params![
                new_id,
                workflow_id,
                e.from,
                e.to,
                e.condition.as_ref().map(serde_json::to_string).transpose()?,
            ],
        )?;
    }
    tx.commit()?;

    Ok(ImportResult {
        workflow_id,
        name: doc.metadata.name,
    })
}
