//! 命令节点 - 执行已注册的命令

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::node::{render_config, stream_event, Node, NodeContext, NodeOutput};
use crate::core::events::StreamKind;

#[derive(Debug, Deserialize)]
struct CmdConfig {
    command_ref: String,
    #[serde(default)]
    param_overrides: Value,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    env: Value,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

pub struct CmdNode;

#[async_trait]
impl Node for CmdNode {
    fn type_id(&self) -> &'static str {
        "cmd"
    }
    fn display_name(&self) -> &'static str {
        "命令"
    }
    fn category(&self) -> &'static str {
        "core"
    }
    fn description(&self) -> &'static str {
        "执行一个已注册的命令"
    }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command_ref"],
            "properties": {
                "command_ref": {
                    "type": "string",
                    "title": "命令 ID 或名称",
                    "description": "已注册的命令（用名称或 UUID）"
                },
                "param_overrides": {
                    "type": "object",
                    "title": "参数覆盖",
                    "description": "覆盖命令的默认参数"
                },
                "cwd": { "type": "string", "title": "工作目录" },
                "env": { "type": "object", "title": "环境变量" },
                "timeout_ms": { "type": "integer", "title": "超时(毫秒)" }
            }
        })
    }

    async fn execute(
        &self,
        ctx: NodeContext,
        config: Value,
    ) -> crate::error::AppResult<NodeOutput> {
        let cfg: CmdConfig = serde_json::from_value(config.clone())
            .map_err(|e| crate::error::AppError::invalid(format!("cmd config 解析失败: {e}")))?;

        // 渲染 config 中的所有字符串
        let interp = build_interp(&ctx);
        let cfg_cmd_ref = render_config(&Value::String(cfg.command_ref.clone()), &interp)?;
        let cfg_cwd = if let Some(c) = &cfg.cwd {
            Some(
                render_config(&Value::String(c.clone()), &interp)?
                    .as_str()
                    .map(String::from)
                    .unwrap_or_default(),
            )
        } else {
            None
        };
        let cfg_env = render_config(&cfg.env, &interp)?;
        let cfg_params = render_config(&cfg.param_overrides, &interp)?;

        // 解析 command_ref 为 ID（支持 name 或 UUID）
        let command_id = resolve_command_id(&ctx, cfg_cmd_ref.as_str().unwrap_or("")).await?;
        let command = crate::storage::models::CommandDetail::load(&ctx.db, &command_id)
            .await?
            .ok_or_else(|| crate::error::AppError::not_found(format!("command:{command_id}")))?;

        // 合并参数: command 默认 + overrides
        let mut merged_params: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for p in &command.params {
            if let Some(dv) = &p.default_value {
                merged_params.insert(p.name.clone(), json_to_string(dv));
            }
        }
        if let Some(obj) = cfg_params.as_object() {
            for (k, v) in obj {
                merged_params.insert(k.clone(), json_to_string(v));
            }
        }

        // 合并参数: command 默认 + overrides
        let mut merged_params: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for p in &command.params {
            if let Some(dv) = &p.default_value {
                merged_params.insert(p.name.clone(), json_to_string(dv));
            }
        }
        if let Some(obj) = cfg_params.as_object() {
            for (k, v) in obj {
                merged_params.insert(k.clone(), json_to_string(v));
            }
        }

        // 渲染模板
        let interp2 = build_interp(&ctx);
        let rendered = crate::core::interpolation::render(&command.version.template, &interp2)?;

        // 调用 Phase 1 的执行器逻辑
        let (program, args) = build_command(&command.command.command_type, &rendered);

        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        if let Some(cwd) = &cfg_cwd {
            if !cwd.is_empty() {
                cmd.current_dir(cwd);
            }
        }
        if let Some(env_obj) = cfg_env.as_object() {
            for (k, v) in env_obj {
                if let Some(s) = v.as_str() {
                    cmd.env(k, s);
                }
            }
        }

        #[cfg(windows)]
        {
            const FLAGS: u32 = 0x0000_0200 | 0x0800_0000;
            cmd.creation_flags(FLAGS);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| crate::error::AppError::other(format!("启动子进程失败: {e}")))?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let stdout_buf = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
        let stderr_buf = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));

        let app1 = ctx.app.clone();
        let app2 = ctx.app.clone();
        let eid1 = ctx.execution_id.clone();
        let nid1 = ctx.node_id.clone();
        let eid2 = ctx.execution_id.clone();
        let nid2 = ctx.node_id.clone();
        let buf1 = stdout_buf.clone();
        let buf2 = stderr_buf.clone();

        let stdout_task = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                buf1.lock().await.push_str(&line);
                buf1.lock().await.push('\n');
                stream_event(&app1, &eid1, &nid1, StreamKind::Stdout, format!("{line}\n"));
            }
        });
        let stderr_task = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                buf2.lock().await.push_str(&line);
                buf2.lock().await.push('\n');
                stream_event(&app2, &eid2, &nid2, StreamKind::Stderr, format!("{line}\n"));
            }
        });

        let start = std::time::Instant::now();
        let result = if let Some(timeout) = cfg.timeout_ms {
            tokio::select! {
                r = child.wait() => Ok(r),
                _ = tokio::time::sleep(std::time::Duration::from_millis(timeout)) => Err("timeout"),
                _ = ctx.cancel.cancelled() => Err("cancelled"),
            }
        } else {
            tokio::select! {
                r = child.wait() => Ok(r),
                _ = ctx.cancel.cancelled() => Err("cancelled"),
            }
        };

        let _ = stdout_task.await;
        let _ = stderr_task.await;
        let _ = start.elapsed();

        let stdout_text = stdout_buf.lock().await.clone();
        let stderr_text = stderr_buf.lock().await.clone();

        match result {
            Ok(Ok(es)) => {
                let code = es.code();
                let status = if code == Some(0) {
                    crate::core::events::NodeStatus::Success
                } else {
                    crate::core::events::NodeStatus::Failed
                };
                Ok(NodeOutput {
                    value: json!({
                        "exit_code": code,
                        "stdout": stdout_text,
                        "stderr": stderr_text,
                    }),
                    stdout: stdout_text,
                    stderr: stderr_text,
                    exit_code: code,
                    status,
                    error: if code != Some(0) {
                        Some(format!("退出码: {:?}", code))
                    } else {
                        None
                    },
                    ..Default::default()
                })
            }
            Ok(Err(e)) => Ok(NodeOutput::failed(format!("wait 失败: {e}"))),
            Err("timeout") => {
                let _ = child.start_kill();
                Ok(NodeOutput {
                    status: crate::core::events::NodeStatus::Timeout,
                    error: Some("执行超时".into()),
                    ..Default::default()
                })
            }
            Err("cancelled") => {
                let _ = child.start_kill();
                Ok(NodeOutput {
                    status: crate::core::events::NodeStatus::Cancelled,
                    error: Some("用户取消".into()),
                    ..Default::default()
                })
            }
            Err(_) => Ok(NodeOutput::failed("未知错误")),
        }
    }
}

fn build_command(
    command_type: &crate::storage::models::CommandType,
    template: &str,
) -> (String, Vec<String>) {
    use crate::storage::models::CommandType;
    match command_type {
        CommandType::Cmd => ("cmd".into(), vec!["/c".into(), template.into()]),
        CommandType::Pwsh => {
            // 同样走探测逻辑，pwsh 优先，没有就降级
            let bin = crate::core::executor::powershell_bin();
            (
                bin.into(),
                vec!["-NoProfile".into(), "-Command".into(), template.into()],
            )
        }
        CommandType::PowerShell => (
            "powershell".into(),
            vec!["-NoProfile".into(), "-Command".into(), template.into()],
        ),
        CommandType::Python => ("python".into(), vec!["-c".into(), template.into()]),
        CommandType::Node => ("node".into(), vec!["-e".into(), template.into()]),
        CommandType::Bash => ("bash".into(), vec!["-c".into(), template.into()]),
        CommandType::Script => (template.to_string(), vec![]),
    }
}

async fn resolve_command_id(
    ctx: &NodeContext,
    name_or_id: &str,
) -> crate::error::AppResult<String> {
    let conn = ctx.db.get()?;

    // 先按 ID 试
    let by_id: Option<String> = conn
        .query_row("SELECT id FROM commands WHERE id = ?1", [name_or_id], |r| {
            r.get(0)
        })
        .ok();
    if let Some(id) = by_id {
        return Ok(id);
    }

    // 再按 name 试
    let by_name: Option<String> = conn
        .query_row(
            "SELECT id FROM commands WHERE name = ?1",
            [name_or_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = by_name {
        return Ok(id);
    }

    Err(crate::error::AppError::not_found(format!(
        "command: {name_or_id}"
    )))
}

fn json_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn build_interp(ctx: &NodeContext) -> crate::core::interpolation::InterpContext {
    let mut i = crate::core::interpolation::InterpContext::new();
    // 工作流参数
    for (k, v) in &ctx.workflow_params {
        i.params.insert(k.clone(), v.clone());
    }
    // 上游节点输出
    for (k, v) in &ctx.upstream_outputs {
        i.node_outputs.insert(k.clone(), v.clone());
    }
    // 系统环境
    for (k, v) in std::env::vars() {
        i.env.insert(k, v);
    }
    i
}
