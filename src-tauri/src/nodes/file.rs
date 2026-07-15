//! 文件节点 - 文件 IO

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

use super::node::{render_config, Node, NodeContext, NodeOutput};
use crate::core::events::NodeStatus;
use crate::core::interpolation::InterpContext;

#[derive(Debug, Deserialize)]
struct FileConfig {
    action: String,
    #[serde(default)]
    src: Option<String>,
    #[serde(default)]
    dst: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // 预留给未来「按 pattern 操作」功能
    pattern: Option<String>,
}

pub struct FileNode;

#[async_trait]
impl Node for FileNode {
    fn type_id(&self) -> &'static str {
        "file"
    }
    fn display_name(&self) -> &'static str {
        "文件"
    }
    fn category(&self) -> &'static str {
        "io"
    }
    fn description(&self) -> &'static str {
        "文件读写操作"
    }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write", "copy", "move", "delete", "exists", "list"],
                    "title": "操作"
                },
                "src": { "type": "string", "title": "源路径" },
                "dst": { "type": "string", "title": "目标路径" },
                "content": { "type": "string", "title": "写入内容", "format": "textarea" },
                "pattern": { "type": "string", "title": "glob 模式 (list 用)" }
            }
        })
    }

    async fn execute(
        &self,
        ctx: NodeContext,
        config: Value,
    ) -> crate::error::AppResult<NodeOutput> {
        let cfg: FileConfig = serde_json::from_value(config.clone())
            .map_err(|e| crate::error::AppError::invalid(format!("file config 解析失败: {e}")))?;

        let mut interp = InterpContext::new();
        for (k, v) in &ctx.workflow_params {
            interp.params.insert(k.clone(), v.clone());
        }
        for (k, v) in &ctx.upstream_outputs {
            interp.node_outputs.insert(k.clone(), v.clone());
        }

        let render_str = |s: &str| -> String {
            render_config(&Value::String(s.to_string()), &interp)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default()
        };

        let src = cfg.src.as_deref().map(render_str).map(PathBuf::from);
        let dst = cfg.dst.as_deref().map(render_str).map(PathBuf::from);

        match cfg.action.as_str() {
            "read" => {
                let p = src.ok_or_else(|| crate::error::AppError::invalid("file.read 需要 src"))?;
                let content = tokio::fs::read_to_string(&p)
                    .await
                    .map_err(|e| crate::error::AppError::other(format!("读取失败: {e}")))?;
                Ok(NodeOutput::success(json!({
                    "content": content,
                    "path": p.to_string_lossy(),
                    "size": content.len(),
                })))
            }
            "write" => {
                let p =
                    dst.ok_or_else(|| crate::error::AppError::invalid("file.write 需要 dst"))?;
                if let Some(parent) = p.parent() {
                    tokio::fs::create_dir_all(parent).await.ok();
                }
                let content = cfg.content.unwrap_or_default();
                tokio::fs::write(&p, &content)
                    .await
                    .map_err(|e| crate::error::AppError::other(format!("写入失败: {e}")))?;
                Ok(NodeOutput::success(json!({
                    "path": p.to_string_lossy(),
                    "size": content.len(),
                })))
            }
            "copy" => {
                let s = src.ok_or_else(|| crate::error::AppError::invalid("file.copy 需要 src"))?;
                let d = dst.ok_or_else(|| crate::error::AppError::invalid("file.copy 需要 dst"))?;
                tokio::fs::create_dir_all(d.parent().unwrap()).await.ok();
                tokio::fs::copy(&s, &d)
                    .await
                    .map_err(|e| crate::error::AppError::other(format!("复制失败: {e}")))?;
                Ok(NodeOutput::success(
                    json!({ "src": s.to_string_lossy(), "dst": d.to_string_lossy() }),
                ))
            }
            "move" => {
                let s = src.ok_or_else(|| crate::error::AppError::invalid("file.move 需要 src"))?;
                let d = dst.ok_or_else(|| crate::error::AppError::invalid("file.move 需要 dst"))?;
                tokio::fs::create_dir_all(d.parent().unwrap()).await.ok();
                tokio::fs::rename(&s, &d)
                    .await
                    .map_err(|e| crate::error::AppError::other(format!("移动失败: {e}")))?;
                Ok(NodeOutput::success(
                    json!({ "src": s.to_string_lossy(), "dst": d.to_string_lossy() }),
                ))
            }
            "delete" => {
                let p =
                    src.ok_or_else(|| crate::error::AppError::invalid("file.delete 需要 src"))?;
                let meta = tokio::fs::metadata(&p).await;
                match meta {
                    Ok(m) if m.is_dir() => tokio::fs::remove_dir_all(&p)
                        .await
                        .map_err(|e| crate::error::AppError::other(format!("删除目录失败: {e}")))?,
                    Ok(_) => tokio::fs::remove_file(&p)
                        .await
                        .map_err(|e| crate::error::AppError::other(format!("删除文件失败: {e}")))?,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Ok(NodeOutput::failed(format!("stat 失败: {e}"))),
                }
                Ok(NodeOutput::success(
                    json!({ "deleted": p.to_string_lossy() }),
                ))
            }
            "exists" => {
                let p =
                    src.ok_or_else(|| crate::error::AppError::invalid("file.exists 需要 src"))?;
                let exists = p.exists();
                Ok(NodeOutput::success(
                    json!({ "exists": exists, "path": p.to_string_lossy() }),
                ))
            }
            "list" => {
                let dir =
                    src.ok_or_else(|| crate::error::AppError::invalid("file.list 需要 src"))?;
                let mut entries = tokio::fs::read_dir(&dir)
                    .await
                    .map_err(|e| crate::error::AppError::other(format!("读取目录失败: {e}")))?;
                let mut files = Vec::new();
                while let Some(e) = entries
                    .next_entry()
                    .await
                    .map_err(|e| crate::error::AppError::other(format!("遍历目录失败: {e}")))?
                {
                    files.push(json!({
                        "name": e.file_name().to_string_lossy(),
                        "is_dir": e.file_type().await.map(|t| t.is_dir()).unwrap_or(false),
                    }));
                }
                Ok(NodeOutput::success(
                    json!({ "files": files, "count": files.len() }),
                ))
            }
            other => Ok(NodeOutput {
                status: NodeStatus::Failed,
                error: Some(format!("未知 file action: {other}")),
                ..Default::default()
            }),
        }
    }
}
