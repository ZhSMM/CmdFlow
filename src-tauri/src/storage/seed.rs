//! 首次启动种子数据
//!
//! 当 commands 表为空时自动注入一批示例命令,涵盖各种 node 类型,
//! 让用户首次打开就能玩,不用从空白开始。
//!
//! - 分 4 个分类: 实用工具 / 网络 / 开发 / AI
//! - 7 条命令: echo / ls / ping / 读文件 / HTTP / AI 问答 / Git 状态
//! - 2 个示例工作流: 每日备份 + HTTP 健康检查
//!
//! 种子只在 commands 表为空时运行,不会覆盖用户的自定义内容。

use std::sync::Arc;

use rusqlite::OptionalExtension;
use serde_json::json;
use uuid::Uuid;

use crate::error::AppResult;
use crate::storage::db::DbPool;
use crate::storage::models::CommandType;

/// 检查 commands 表是否为空
pub fn is_empty(db: &DbPool) -> AppResult<bool> {
    let conn = db.get()?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM commands", [], |r| r.get(0))?;
    Ok(count == 0)
}

/// 注入种子数据。幂等: commands 不空时直接返回。
pub fn run(db: &DbPool) -> AppResult<()> {
    if !is_empty(db)? {
        tracing::debug!("commands 表已有数据, 跳过 seed");
        return Ok(());
    }
    tracing::info!("首次启动, 注入种子命令...");

    seed_categories(db)?;
    seed_commands(db)?;
    seed_workflows(db)?;

    tracing::info!("种子数据注入完成 (4 分类, 7 命令, 2 工作流)");
    Ok(())
}

// ==================== 分类 ====================

struct SeedCategory {
    id: &'static str,
    name: &'static str,
    icon: &'static str,
    sort_order: i32,
}

const CATEGORIES: &[SeedCategory] = &[
    SeedCategory { id: "cat_util",  name: "实用工具", icon: "Wrench",      sort_order: 1 },
    SeedCategory { id: "cat_net",   name: "网络",     icon: "Globe",       sort_order: 2 },
    SeedCategory { id: "cat_dev",   name: "开发",     icon: "Code",        sort_order: 3 },
    SeedCategory { id: "cat_ai",    name: "AI",       icon: "Sparkles",    sort_order: 4 },
];

fn seed_categories(db: &DbPool) -> AppResult<()> {
    let conn = db.get()?;
    let now = chrono::Utc::now().timestamp();
    for c in CATEGORIES {
        conn.execute(
            "INSERT OR IGNORE INTO categories (id, parent_id, name, icon, sort_order, created_at)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5)",
            rusqlite::params![c.id, c.name, c.icon, c.sort_order, now],
        )?;
    }
    Ok(())
}

// ==================== 命令 ====================

struct SeedCommand {
    name: &'static str,
    description: &'static str,
    category_id: &'static str,
    command_type: CommandType,
    template: &'static str,
    working_dir: Option<&'static str>,
    timeout_ms: Option<i64>,
    tags: Vec<&'static str>,
    params: Vec<SeedParam>,
}

struct SeedParam {
    name: &'static str,
    label: &'static str,
    param_type: &'static str,
    required: bool,
    default_value: Option<serde_json::Value>,
    description: Option<&'static str>,
    sort_order: i32,
}

fn seed_commands_data() -> Vec<SeedCommand> {
    vec![
    SeedCommand {
        name: "echo 问候",
        description: "最简单的 echo 命令,展示参数插值 ({{name}} 会被替换)",
        category_id: "cat_util",
        command_type: CommandType::Pwsh,
        template: "Write-Host \"Hello, {{name}}!\"",
        working_dir: None,
        timeout_ms: Some(5000),
        tags: vec!["示例", "基础"],
        params: vec![
            SeedParam {
                name: "name",
                label: "问候对象",
                param_type: "text",
                required: false,
                default_value: Some(json!("World")),
                description: Some("要问候的人或物"),
                sort_order: 0,
            },
        ],
    },
    SeedCommand {
        name: "列出当前目录",
        description: "ls 当前目录内容(Windows 用 PowerShell Get-ChildItem)",
        category_id: "cat_util",
        command_type: CommandType::Pwsh,
        template: "Get-ChildItem -Force | Format-Table Name, Length, LastWriteTime -AutoSize",
        working_dir: None,
        timeout_ms: Some(10000),
        tags: vec!["示例", "文件"],
        params: vec![],
    },
    SeedCommand {
        name: "Ping 主机",
        description: "ping 一个地址 4 次,带参数 {{host}}",
        category_id: "cat_net",
        command_type: CommandType::Cmd,
        template: "ping -n 4 {{host}}",
        working_dir: None,
        timeout_ms: Some(30000),
        tags: vec!["示例", "网络"],
        params: vec![
            SeedParam {
                name: "host",
                label: "目标主机",
                param_type: "text",
                required: true,
                default_value: Some(json!("1.1.1.1")),
                description: Some("IP 或域名"),
                sort_order: 0,
            },
        ],
    },
    SeedCommand {
        name: "读取文件",
        description: "读取本地文件前 50 行,适合快速预览",
        category_id: "cat_util",
        command_type: CommandType::Pwsh,
        template: "if (-not (Test-Path \"{{path}}\")) { throw \"文件不存在: {{path}}\" }; Get-Content \"{{path}}\" -Head 50",
        working_dir: None,
        timeout_ms: Some(5000),
        tags: vec!["示例", "文件"],
        params: vec![
            SeedParam {
                name: "path",
                label: "文件路径",
                param_type: "file",
                required: true,
                default_value: None,
                description: Some("要读取的文件绝对路径"),
                sort_order: 0,
            },
        ],
    },
    SeedCommand {
        name: "HTTP GET",
        description: "发个 HTTP GET,看返回内容。参数 {{url}}",
        category_id: "cat_net",
        command_type: CommandType::Pwsh,
        template: "try { Invoke-WebRequest -Uri \"{{url}}\" -UseBasicParsing -TimeoutSec 15 | Select-Object -ExpandProperty Content } catch { Write-Error $_.Exception.Message }",
        working_dir: None,
        timeout_ms: Some(20000),
        tags: vec!["示例", "http", "网络"],
        params: vec![
            SeedParam {
                name: "url",
                label: "URL",
                param_type: "text",
                required: true,
                default_value: Some(json!("https://wttr.in?format=3")),
                description: Some("完整的 URL,带 https://"),
                sort_order: 0,
            },
        ],
    },
    SeedCommand {
        name: "Git 状态",
        description: "git status --short 简略版,适合快速检查仓库状态",
        category_id: "cat_dev",
        command_type: CommandType::Pwsh,
        template: "if (-not (Test-Path .git)) { Write-Host \"当前目录不是 git 仓库\" -ForegroundColor Yellow; return }; git status --short; git log --oneline -5",
        working_dir: None,
        timeout_ms: Some(10000),
        tags: vec!["示例", "git"],
        params: vec![],
    },
    SeedCommand {
        name: "AI 问答",
        description: "调 OpenAI 兼容 API 回答问题。需在设置里配 API Key 引用(api_key_ref)",
        category_id: "cat_ai",
        command_type: CommandType::Script,
        template: "openai_chat",
        working_dir: None,
        timeout_ms: Some(60000),
        tags: vec!["示例", "ai"],
        params: vec![
            SeedParam {
                name: "question",
                label: "问题",
                param_type: "textarea",
                required: true,
                default_value: Some(json!("用一句话解释什么是 CmdFlow")),
                description: Some("想问 AI 的内容"),
                sort_order: 0,
            },
            SeedParam {
                name: "model",
                label: "模型",
                param_type: "text",
                required: false,
                default_value: Some(json!("gpt-4o-mini")),
                description: Some("OpenAI 兼容模型名"),
                sort_order: 1,
            },
        ],
    },
    ]
}

fn seed_commands(db: &DbPool) -> AppResult<()> {
    let conn = db.get()?;
    let now = chrono::Utc::now().timestamp();

    for c in seed_commands_data() {
        let id = Uuid::new_v4().to_string();
        let tags_json = serde_json::to_string(&c.tags)?;

        // 1. commands 表
        let category_name: Option<&str> = CATEGORIES
            .iter()
            .find(|cat| cat.id == c.category_id)
            .map(|cat| cat.name);
        conn.execute(
            "INSERT INTO commands (id, name, description, category, category_id, type, current_ver, tags, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?8)",
            rusqlite::params![
                &id,
                c.name,
                c.description,
                category_name,
                c.category_id,
                c.command_type.as_str(),
                &tags_json,
                now,
            ],
        )?;

        // 2. command_versions 表
        let env_json: Option<String> = None;
        conn.execute(
            "INSERT INTO command_versions (command_id, version, template, working_dir, env, timeout_ms, shell, created_at)
             VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                &id,
                c.template,
                c.working_dir,
                env_json,
                c.timeout_ms,
                Option::<String>::None,
                now,
            ],
        )?;

        // 3. params 表
        let ver_id: i64 = conn.query_row(
            "SELECT id FROM command_versions WHERE command_id = ?1 AND version = 1",
            [&id],
            |r| r.get(0),
        )?;
        for p in c.params {
            let default_str = p.default_value.as_ref().map(serde_json::to_string).transpose()?;
            conn.execute(
                "INSERT INTO params (id, command_ver_id, name, label, type, required, default_value, options, validation, sensitive, description, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL, 0, ?8, ?9)",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    ver_id,
                    p.name,
                    p.label,
                    p.param_type,
                    p.required as i32,
                    default_str,
                    p.description,
                    p.sort_order,
                ],
            )?;
        }
    }

    Ok(())
}

// ==================== 工作流 ====================

fn seed_workflows(db: &DbPool) -> AppResult<()> {
    use crate::commands::yaml_io;

    let workflows: &[(&str, &str)] = &[
        (
            "示例 - 每日备份",
            include_str!("../../templates/daily-backup.yaml"),
        ),
        (
            "示例 - HTTP 健康检查",
            include_str!("../../templates/http-healthcheck.yaml"),
        ),
    ];

    for (name, yaml) in workflows {
        let _ = name;
        let doc: yaml_io::WorkflowYaml = serde_yaml::from_str(yaml)
            .map_err(|e| crate::error::AppError::other(format!("seed 模板 {name} 解析失败: {e}")))?;
        let wf_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        let conn = db.get()?;

        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM workflows WHERE name = ?1 LIMIT 1",
                [&doc.metadata.name],
                |r| r.get(0),
            )
            .optional()?;
        if existing.is_some() {
            continue;
        }

        conn.execute(
            "INSERT INTO workflows (id, name, description, enabled, trigger_type, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5)",
            rusqlite::params![
                wf_id,
                doc.metadata.name,
                doc.metadata.description,
                doc.metadata.trigger.unwrap_or_else(|| "manual".to_string()),
                now,
            ],
        )?;

        let tx = conn.unchecked_transaction()?;
        let mut id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for (i, n) in doc.nodes.iter().enumerate() {
            let nid = format!("n_{}", Uuid::new_v4().to_string().replace('-', ""));
            id_map.insert(n.id.clone(), nid.clone());
            let (px, py) = n.position.as_ref().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
            tx.execute(
                "INSERT INTO nodes (id, workflow_id, type, command_id, config, position_x, position_y, sort_order)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    nid, wf_id, n.node_type, n.command_ref,
                    serde_json::to_string(&n.config)?,
                    px, py, i as i32,
                ],
            )?;
        }
        for e in &doc.edges {
            let from_new = id_map.get(&e.from).cloned().unwrap_or_else(|| e.from.clone());
            let to_new = id_map.get(&e.to).cloned().unwrap_or_else(|| e.to.clone());
            let eid = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO edges (id, workflow_id, source_node, target_node, source_port, target_port, condition)
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5)",
                rusqlite::params![
                    eid, wf_id, from_new, to_new,
                    e.condition.as_ref().map(serde_json::to_string).transpose()?,
                ],
            )?;
        }
        tx.commit()?;
    }

    Ok(())
}

/// 暴露给 AppState 调用的入口
pub fn maybe_seed(db: &Arc<DbPool>) {
    if let Err(e) = run(db) {
        tracing::warn!("种子数据注入失败: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db;

    fn fresh_db() -> Arc<DbPool> {
        // 内存数据库,每次独立
        Arc::new(db::open_pool_memory().expect("打开内存 db 失败"))
    }

    #[test]
    fn seed_runs_on_empty_db() {
        let db = fresh_db();
        // 先跑迁移
        db::run_migrations(&db).expect("迁移失败");

        assert!(is_empty(&db).unwrap());
        run(&db).expect("seed 失败");
        assert!(!is_empty(&db).unwrap(), "seed 后 commands 不应为空");

        // 至少应包含我们注册的 7 条
        let conn = db.get().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM commands", [], |r| r.get(0)).unwrap();
        assert!(count >= 7, "应至少有 7 条命令, 实际 {}", count);
    }

    #[test]
    fn seed_is_idempotent() {
        let db = fresh_db();
        db::run_migrations(&db).unwrap();

        run(&db).unwrap();
        let after_first = db.get().unwrap().query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM commands", [], |r| r.get(0)
        ).unwrap();

        // 第二次跑应直接跳过 (is_empty = false)
        run(&db).unwrap();
        let after_second = db.get().unwrap().query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM commands", [], |r| r.get(0)
        ).unwrap();

        assert_eq!(after_first, after_second, "重复 seed 不应插入");
    }

    #[test]
    fn seed_includes_categories() {
        let db = fresh_db();
        db::run_migrations(&db).unwrap();
        run(&db).unwrap();

        let conn = db.get().unwrap();
        let cat_count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0)).unwrap();
        assert!(cat_count >= 4, "应至少有 4 个分类, 实际 {}", cat_count);

        let util_exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM categories WHERE name = '实用工具'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(util_exists, 1, "应包含「实用工具」分类");
    }

    #[test]
    fn seed_includes_workflows() {
        let db = fresh_db();
        db::run_migrations(&db).unwrap();
        run(&db).unwrap();

        let conn = db.get().unwrap();
        let wf_count: i64 = conn.query_row("SELECT COUNT(*) FROM workflows", [], |r| r.get(0)).unwrap();
        assert!(wf_count >= 2, "应至少有 2 个工作流, 实际 {}", wf_count);
    }

    #[test]
    fn seed_runs_on_empty_db_v2() {
        let db = fresh_db();
        db::run_migrations(&db).expect("迁移失败");
        seed_categories(&db).expect("分类失败");
        let result = seed_commands(&db);
        eprintln!("seed_commands 结果: {:?}", result);
    }

    #[test]
    fn minimal_insert_test() {
        let db = fresh_db();
        db::run_migrations(&db).unwrap();
        let conn = db.get().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO categories (id, parent_id, name, icon, sort_order, created_at) VALUES ('cat_test', NULL, 'Test', 'X', 1, ?1)",
            rusqlite::params![now],
        ).expect("insert category failed");
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        conn.execute(
            "INSERT INTO commands (id, name, description, category, category_id, type, current_ver, tags, created_at, updated_at)
             VALUES ('cmd_test', 'Test', 'desc', 'Test', 'cat_test', 'pwsh', 1, '[]', ?1, ?1)",
            rusqlite::params![now],
        ).expect("insert command failed");
    }
}
