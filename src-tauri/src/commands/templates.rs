//! 工作流模板市场 (Phase 9.2)
//!
//! 模板以 YAML 文件形式打包在 `src-tauri/templates/` 下,
//! 通过 `include_str!` 编译进二进制,运行时直接读静态字符串。
//! 元数据从 `manifest.yaml` 解析,模板正文从对应 `*.yaml` 读。

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::commands::yaml_io::{self, ImportResult};

// ==================== 嵌入的模板文件 ====================

const MANIFEST: &str = include_str!("../../templates/manifest.yaml");
const GIT_DEPLOY: &str = include_str!("../../templates/git-deploy.yaml");
const DOCKER_BUILD_PUSH: &str = include_str!("../../templates/docker-build-push.yaml");
const LOG_CLEANUP: &str = include_str!("../../templates/log-cleanup.yaml");
const HTTP_DATA_PIPELINE: &str = include_str!("../../templates/http-data-pipeline.yaml");
const DAILY_BACKUP: &str = include_str!("../../templates/daily-backup.yaml");
const HTTP_HEALTHCHECK: &str = include_str!("../../templates/http-healthcheck.yaml");

// ==================== 内部类型 ====================

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    id: String,
    category: String,
    #[serde(default)]
    tags: Vec<String>,
    icon: String,
}

#[derive(Debug, Serialize)]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub icon: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub builtin: bool,
}

struct TemplateEntry {
    yaml: &'static str,
}

fn builtin_templates() -> Vec<(&'static str, TemplateEntry)> {
    vec![
        ("git-deploy", TemplateEntry { yaml: GIT_DEPLOY }),
        ("docker-build-push", TemplateEntry { yaml: DOCKER_BUILD_PUSH }),
        ("log-cleanup", TemplateEntry { yaml: LOG_CLEANUP }),
        ("http-data-pipeline", TemplateEntry { yaml: HTTP_DATA_PIPELINE }),
        ("daily-backup", TemplateEntry { yaml: DAILY_BACKUP }),
        ("http-healthcheck", TemplateEntry { yaml: HTTP_HEALTHCHECK }),
    ]
}

// ==================== 解析 ====================

fn load_manifest() -> AppResult<Vec<ManifestEntry>> {
    serde_yaml::from_str(MANIFEST)
        .map_err(|e| AppError::other(format!("模板清单解析失败: {e}")))
}

fn build_summary(yaml: &str, manifest: &ManifestEntry) -> AppResult<TemplateSummary> {
    let parsed: yaml_io::WorkflowYaml = serde_yaml::from_str(yaml)
        .map_err(|e| AppError::other(format!("模板 [{}] 解析失败: {e}", manifest.id)))?;
    Ok(TemplateSummary {
        id: manifest.id.clone(),
        name: parsed.metadata.name,
        description: parsed.metadata.description.unwrap_or_default(),
        category: manifest.category.clone(),
        tags: manifest.tags.clone(),
        icon: manifest.icon.clone(),
        node_count: parsed.nodes.len(),
        edge_count: parsed.edges.len(),
        builtin: true,
    })
}

fn find_yaml(id: &str) -> AppResult<&'static str> {
    for (key, entry) in builtin_templates() {
        if key == id {
            return Ok(entry.yaml);
        }
    }
    Err(AppError::not_found(format!("template: {id}")))
}

// ==================== Tauri 命令 ====================

#[tauri::command]
pub async fn list_templates() -> AppResult<Vec<TemplateSummary>> {
    let manifest = load_manifest()?;
    let mut out = Vec::with_capacity(manifest.len());
    for entry in manifest {
        let yaml = find_yaml(&entry.id)?;
        out.push(build_summary(yaml, &entry)?);
    }
    Ok(out)
}

#[tauri::command]
pub async fn get_template(id: String) -> AppResult<String> {
    let yaml = find_yaml(&id)?;
    Ok(yaml.to_string())
}

#[tauri::command]
pub async fn import_template(
    state: State<'_, AppState>,
    id: String,
    new_name: Option<String>,
) -> AppResult<ImportResult> {
    let yaml = find_yaml(&id)?;
    // 如果给了新名字,改一下 metadata.name
    let yaml = if let Some(name) = new_name {
        if name.trim().is_empty() {
            yaml.to_string()
        } else {
            let mut doc: yaml_io::WorkflowYaml = serde_yaml::from_str(yaml)
                .map_err(|e| AppError::other(format!("模板解析失败: {e}")))?;
            doc.metadata.name = name.trim().to_string();
            serde_yaml::to_string(&doc)
                .map_err(|e| AppError::other(format!("YAML 序列化失败: {e}")))?
        }
    } else {
        yaml.to_string()
    };

    yaml_io::import_workflow(state, yaml).await
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses() {
        let m = load_manifest().expect("manifest 应可解析");
        assert!(!m.is_empty(), "manifest 不应为空");
        for entry in &m {
            assert!(!entry.id.is_empty(), "template id 不能为空");
            assert!(!entry.category.is_empty(), "template category 不能为空");
            assert!(!entry.icon.is_empty(), "template icon 不能为空");
        }
    }

    #[test]
    fn all_builtin_templates_parse() {
        let manifest = load_manifest().expect("manifest 应可解析");
        let manifest_ids: Vec<&str> = manifest.iter().map(|m| m.id.as_str()).collect();

        for (id, entry) in builtin_templates() {
            assert!(
                manifest_ids.contains(&id),
                "模板 [{}] 未在 manifest 中注册",
                id
            );
            let doc: yaml_io::WorkflowYaml = serde_yaml::from_str(entry.yaml)
                .unwrap_or_else(|e| panic!("模板 [{}] YAML 解析失败: {}", id, e));
            assert_eq!(doc.kind, "workflow", "模板 [{}] kind 必须是 workflow", id);
            assert!(!doc.metadata.name.is_empty(), "模板 [{}] 名称不能为空", id);
            assert!(!doc.nodes.is_empty(), "模板 [{}] 必须有至少一个节点", id);
            // 验证边的 source/target 都指向真实节点
            let node_ids: std::collections::HashSet<&str> =
                doc.nodes.iter().map(|n| n.id.as_str()).collect();
            for e in &doc.edges {
                assert!(
                    node_ids.contains(e.from.as_str()),
                    "模板 [{}] 边的 from 节点不存在: {}",
                    id,
                    e.from
                );
                assert!(
                    node_ids.contains(e.to.as_str()),
                    "模板 [{}] 边的 to 节点不存在: {}",
                    id,
                    e.to
                );
            }
        }
    }

    #[test]
    fn list_templates_returns_all() {
        // 用 tokio runtime 跑 async 函数
        let rt = tokio::runtime::Runtime::new().unwrap();
        let res = rt.block_on(list_templates()).expect("list_templates 应成功");
        let manifest_count = load_manifest().unwrap().len();
        assert_eq!(res.len(), manifest_count, "列表数量应等于 manifest 数量");
        for t in &res {
            assert!(!t.name.is_empty());
            assert!(!t.icon.is_empty());
            assert!(t.node_count > 0, "模板 [{}] 必须有节点", t.id);
        }
    }
}
