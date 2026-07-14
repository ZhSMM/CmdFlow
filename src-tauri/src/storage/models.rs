//! 数据模型
//!
//! 持久化结构与 IPC DTO 分开：
//! - 内部 Model:  与数据库 schema 严格对应
//! - DTO:        通过 `From` 转换，提供给前端

use std::sync::Arc;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::storage::db::DbPool;

// ==================== 命令库 ====================

/// 命令类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CommandType {
    Cmd,
    /// PowerShell 7+ (`pwsh.exe`)，找不到时自动降级到 Windows PowerShell 5.1
    Pwsh,
    /// 明确指定 Windows PowerShell 5.1 (`powershell.exe`)，老机器/Sever Core 才有
    PowerShell,
    Python,
    Node,
    Bash,
    Script,
}

impl CommandType {
    pub fn as_str(self) -> &'static str {
        match self {
            CommandType::Cmd => "cmd",
            CommandType::Pwsh => "pwsh",
            CommandType::PowerShell => "powershell",
            CommandType::Python => "python",
            CommandType::Node => "node",
            CommandType::Bash => "bash",
            CommandType::Script => "script",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "cmd" => Self::Cmd,
            "pwsh" => Self::Pwsh,
            "powershell" => Self::PowerShell,
            "python" => Self::Python,
            "node" => Self::Node,
            "bash" => Self::Bash,
            "script" => Self::Script,
            _ => return None,
        })
    }
}

/// 命令库 Model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    #[serde(rename = "type")]
    pub command_type: CommandType,
    pub current_ver: i32,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 命令版本 Model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandVersion {
    pub id: i64,
    pub command_id: String,
    pub version: i32,
    pub template: String,
    pub working_dir: Option<String>,
    pub env: Option<serde_json::Value>,
    pub timeout_ms: Option<i64>,
    pub shell: Option<String>,
    pub note: Option<String>,
    pub created_at: i64,
}

/// 参数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub id: String,
    pub command_ver_id: i64,
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
    pub options: Option<serde_json::Value>,
    pub validation: Option<serde_json::Value>,
    pub sensitive: bool,
    pub description: Option<String>,
    pub sort_order: i32,
}

/// 命令 + 当前版本 + 参数 (前端需要的复合结构)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDetail {
    #[serde(flatten)]
    pub command: Command,
    pub version: CommandVersion,
    pub params: Vec<Param>,
}

impl CommandDetail {
    /// 从数据库加载命令详情
    pub async fn load(db: &Arc<DbPool>, id: &str) -> AppResult<Option<Self>> {
        let db = db.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || -> AppResult<Option<Self>> {
            let conn = db.get()?;

            let cmd: Option<Command> = conn
                .query_row(
                    "SELECT id, name, description, category, type, current_ver, tags, created_at, updated_at
                     FROM commands WHERE id = ?1",
                    [&id],
                    |r| {
                        let tags_json: String = r.get(6)?;
                        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                        Ok(Command {
                            id: r.get(0)?,
                            name: r.get(1)?,
                            description: r.get(2)?,
                            category: r.get(3)?,
                            command_type: CommandType::parse(&r.get::<_, String>(4)?)
                                .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
                            current_ver: r.get(5)?,
                            tags,
                            created_at: r.get(7)?,
                            updated_at: r.get(8)?,
                        })
                    },
                )
                .optional()?;

            let cmd = match cmd {
                Some(c) => c,
                None => return Ok(None),
            };

            let version: CommandVersion = conn.query_row(
                "SELECT id, command_id, version, template, working_dir, env, timeout_ms, shell, note, created_at
                 FROM command_versions WHERE command_id = ?1 AND version = ?2",
                rusqlite::params![&id, cmd.current_ver],
                |r| Ok(CommandVersion {
                    id: r.get(0)?,
                    command_id: r.get(1)?,
                    version: r.get(2)?,
                    template: r.get(3)?,
                    working_dir: r.get(4)?,
                    env: r.get::<_, Option<String>>(5)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    timeout_ms: r.get(6)?,
                    shell: r.get(7)?,
                    note: r.get(8)?,
                    created_at: r.get(9)?,
                }),
            )?;

            let mut stmt = conn.prepare(
                "SELECT id, command_ver_id, name, label, type, required, default_value, options, validation, sensitive, description, sort_order
                 FROM params WHERE command_ver_id = ?1 ORDER BY sort_order, name",
            )?;
            let params_iter = stmt.query_map([version.id], |r| {
                Ok(Param {
                    id: r.get(0)?,
                    command_ver_id: r.get(1)?,
                    name: r.get(2)?,
                    label: r.get(3)?,
                    param_type: r.get(4)?,
                    required: r.get(5)?,
                    default_value: r.get::<_, Option<String>>(6)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    options: r.get::<_, Option<String>>(7)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    validation: r.get::<_, Option<String>>(8)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    sensitive: r.get(9)?,
                    description: r.get(10)?,
                    sort_order: r.get(11)?,
                })
            })?;
            let mut params_vec = Vec::new();
            for p in params_iter {
                params_vec.push(p?);
            }

            Ok(Some(CommandDetail {
                command: cmd,
                version,
                params: params_vec,
            }))
        })
        .await
        .map_err(|e| AppError::other(format!("加载命令失败: {e}")))?
    }
}

// ==================== 工作流 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub trigger_type: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub workflow_id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub command_id: Option<String>,
    pub config: serde_json::Value,
    pub position_x: f64,
    pub position_y: f64,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub id: String,
    pub workflow_id: String,
    pub source_node: String,
    pub target_node: String,
    pub source_port: Option<String>,
    pub target_port: Option<String>,
    pub condition: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDetail {
    #[serde(flatten)]
    pub workflow: Workflow,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
}

// ==================== 分类树 (Phase 7) ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub created_at: i64,
}

/// 树节点: 分类 + 子分类 + 命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub subcategories: Vec<CategoryNode>,
    pub commands: Vec<Command>,
}

// ==================== 收藏 (Phase 7) ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Favorite {
    pub command_id: String,
    pub sort_order: i32,
    pub created_at: i64,
    /// join 出来的命令详情 (前端用)
    pub command: Option<Command>,
}

// ==================== 插件 (Phase 7) ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub format: String, // 'js' | 'wasm'
    pub entry: String,
    pub manifest: String,
    pub enabled: bool,
    pub installed_at: i64,
    pub updated_at: i64,
}

impl WorkflowDetail {
    /// 加载工作流完整详情（含 nodes 和 edges）
    pub fn load_full(db: &DbPool, id: &str) -> AppResult<Option<Self>> {
        let conn = db.get()?;

        let workflow: Option<Workflow> = conn
            .query_row(
                "SELECT id, name, description, enabled, trigger_type, created_at, updated_at
                 FROM workflows WHERE id = ?1",
                [&id],
                |r| Ok(Workflow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    enabled: r.get(3)?,
                    trigger_type: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                }),
            )
            .ok();
        let workflow = match workflow {
            Some(w) => w,
            None => return Ok(None),
        };

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
                    config: r.get::<_, String>(4)
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
                    condition: r.get::<_, Option<String>>(6)
                        .ok()
                        .flatten()
                        .and_then(|s| serde_json::from_str(&s).ok()),
                })
            })?
            .filter_map(Result::ok)
            .collect();

        Ok(Some(WorkflowDetail { workflow, nodes, edges }))
    }
}
