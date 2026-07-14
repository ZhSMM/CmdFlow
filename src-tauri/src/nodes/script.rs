//! 脚本节点 - 直接执行一段脚本

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::node::{render_config, Node, NodeContext, NodeOutput};
use crate::core::events::NodeStatus;
use crate::core::interpolation::InterpContext;

#[derive(Debug, Deserialize)]
struct ScriptConfig {
    language: String,
    code: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    env: Value,
}

pub struct ScriptNode;

#[async_trait]
impl Node for ScriptNode {
    fn type_id(&self) -> &'static str { "script" }
    fn display_name(&self) -> &'static str { "脚本" }
    fn category(&self) -> &'static str { "core" }
    fn description(&self) -> &'static str { "直接执行一段脚本 (python/node/bash/pwsh)" }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["language", "code"],
            "properties": {
                "language": {
                    "type": "string",
                    "enum": ["python", "node", "bash", "pwsh"],
                    "title": "语言"
                },
                "code": { "type": "string", "title": "代码", "format": "textarea" },
                "timeout_ms": { "type": "integer", "title": "超时(毫秒)" },
                "cwd": { "type": "string", "title": "工作目录" },
                "env": { "type": "object", "title": "环境变量" }
            }
        })
    }

    async fn execute(&self, ctx: NodeContext, config: Value) -> crate::error::AppResult<NodeOutput> {
        let cfg: ScriptConfig = serde_json::from_value(config.clone())
            .map_err(|e| crate::error::AppError::invalid(format!("script config 解析失败: {e}")))?;

        let interp = InterpContext::new();
        let _ = &interp; // 暂不插值，避免破坏脚本结构
        let cfg = cfg;
        let _ = render_config(&json!({}), &interp);

        let (program, args) = match cfg.language.as_str() {
            "python" => ("python".to_string(), vec!["-c".to_string(), cfg.code.clone()]),
            "node" => ("node".to_string(), vec!["-e".to_string(), cfg.code.clone()]),
            "bash" => ("bash".to_string(), vec!["-c".to_string(), cfg.code.clone()]),
            "pwsh" => ("pwsh".to_string(), vec!["-NoProfile".to_string(), "-Command".to_string(), cfg.code.clone()]),
            other => return Ok(NodeOutput::failed(format!("不支持的脚本语言: {other}"))),
        };

        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        if let Some(cwd) = &cfg.cwd {
            if !cwd.is_empty() {
                cmd.current_dir(cwd);
            }
        }
        if let Some(env_obj) = cfg.env.as_object() {
            for (k, v) in env_obj {
                if let Some(s) = v.as_str() {
                    cmd.env(k, s);
                }
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const FLAGS: u32 = 0x0000_0200 | 0x0800_0000;
            cmd.creation_flags(FLAGS);
        }

        let mut child = cmd.spawn().map_err(|e|
            crate::error::AppError::other(format!("启动脚本失败: {e}")))?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let app1 = ctx.app.clone();
        let app2 = ctx.app.clone();
        let eid1 = ctx.execution_id.clone();
        let nid1 = ctx.node_id.clone();
        let eid2 = ctx.execution_id.clone();
        let nid2 = ctx.node_id.clone();

        let stdout_buf = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
        let stderr_buf = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
        let buf1 = stdout_buf.clone();
        let buf2 = stderr_buf.clone();

        let stdout_task = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        buf1.lock().await.push_str(&line);
                        buf1.lock().await.push('\n');
                        crate::nodes::node::stream_event(&app1, &eid1, &nid1,
                            crate::core::events::StreamKind::Stdout, format!("{line}\n"));
                    }
                    _ => break,
                }
            }
        });
        let stderr_task = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(stderr).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        buf2.lock().await.push_str(&line);
                        buf2.lock().await.push('\n');
                        crate::nodes::node::stream_event(&app2, &eid2, &nid2,
                            crate::core::events::StreamKind::Stderr, format!("{line}\n"));
                    }
                    _ => break,
                }
            }
        });

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

        let stdout_text = stdout_buf.lock().await.clone();
        let stderr_text = stderr_buf.lock().await.clone();

        match result {
            Ok(Ok(es)) => {
                let code = es.code();
                Ok(NodeOutput {
                    value: json!({
                        "exit_code": code,
                        "stdout": stdout_text,
                        "stderr": stderr_text,
                    }),
                    stdout: stdout_text,
                    stderr: stderr_text,
                    exit_code: code,
                    status: if code == Some(0) { NodeStatus::Success } else { NodeStatus::Failed },
                    error: if code != Some(0) { Some(format!("退出码: {:?}", code)) } else { None },
                    ..Default::default()
                })
            }
            Ok(Err(e)) => Ok(NodeOutput::failed(format!("wait 失败: {e}"))),
            Err("timeout") => {
                let _ = child.start_kill();
                Ok(NodeOutput {
                    status: NodeStatus::Timeout,
                    error: Some("脚本超时".into()),
                    ..Default::default()
                })
            }
            Err("cancelled") => {
                let _ = child.start_kill();
                Ok(NodeOutput {
                    status: NodeStatus::Cancelled,
                    error: Some("用户取消".into()),
                    ..Default::default()
                })
            }
            Err(_) => Ok(NodeOutput::failed("未知错误")),
        }
    }
}
