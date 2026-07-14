//! AI 节点 - 调用 LLM
//!
//! Phase 4 简化版:支持 OpenAI 兼容 API。
//! Phase 5+ 可扩展 Anthropic / Ollama / 自定义 endpoint。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

use super::node::{Node, NodeContext, NodeOutput};
use crate::core::events::NodeStatus;

#[derive(Debug, Deserialize)]
struct AiConfig {
    /// 兼容: openai | anthropic | ollama | custom
    provider: String,
    model: String,
    #[serde(default)]
    system: Option<String>,
    prompt: String,
    #[serde(default = "default_temp")]
    temperature: f32,
    #[serde(default)]
    api_key_ref: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

fn default_temp() -> f32 { 0.7 }

pub struct AiNode;

#[async_trait]
impl Node for AiNode {
    fn type_id(&self) -> &'static str { "ai" }
    fn display_name(&self) -> &'static str { "AI" }
    fn category(&self) -> &'static str { "io" }
    fn description(&self) -> &'static str { "调用大模型 (OpenAI 兼容)" }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["provider", "model", "prompt"],
            "properties": {
                "provider": { "type": "string", "enum": ["openai", "anthropic", "ollama", "custom"], "title": "服务" },
                "model": { "type": "string", "title": "模型" },
                "system": { "type": "string", "title": "系统提示", "format": "textarea" },
                "prompt": { "type": "string", "title": "用户提示", "format": "textarea" },
                "temperature": { "type": "number", "title": "温度", "minimum": 0, "maximum": 2 },
                "api_key_ref": { "type": "string", "title": "API Key 引用 (keyring 名)" },
                "base_url": { "type": "string", "title": "自定义 endpoint" },
                "timeout_ms": { "type": "integer", "title": "超时(毫秒)" }
            }
        })
    }

    async fn execute(&self, ctx: NodeContext, config: Value) -> crate::error::AppResult<NodeOutput> {
        let cfg: AiConfig = serde_json::from_value(config)
            .map_err(|e| crate::error::AppError::invalid(format!("ai config 解析失败: {e}")))?;

        // 渲染 prompt
        let prompt = render(&cfg.prompt, &ctx);

        // 取 API key: 先 keyring, 再环境变量
        let api_key = if let Some(key_ref) = &cfg.api_key_ref {
            read_keyring(key_ref).unwrap_or_default()
        } else {
            String::new()
        };
        if api_key.is_empty() {
            if let Ok(env_key) = std::env::var("OPENAI_API_KEY") {
                let _ = env_key;
            }
        }

        let base_url = cfg.base_url.clone().unwrap_or_else(|| {
            match cfg.provider.as_str() {
                "openai" => "https://api.openai.com/v1".to_string(),
                "ollama" => "http://localhost:11434/v1".to_string(),
                "anthropic" => "https://api.anthropic.com/v1".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            }
        });

        let client = Client::builder()
            .timeout(Duration::from_millis(cfg.timeout_ms.unwrap_or(60_000)))
            .build()
            .map_err(|e| crate::error::AppError::other(format!("http client: {e}")))?;

        crate::nodes::node::stream_event(&ctx.app, &ctx.execution_id, &ctx.node_id,
            crate::core::events::StreamKind::System,
            format!("🤖 调用 {}/{}...\n", cfg.provider, cfg.model));

        // Phase 4 简化: 走 OpenAI 兼容 /chat/completions
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

        let mut body = json!({
            "model": cfg.model,
            "messages": build_messages(cfg.system.as_deref(), &prompt),
            "temperature": cfg.temperature,
        });

        if !api_key.is_empty() {
            // 通过 env 注入到 client header
            // 这里用 Bearer
        }

        let mut req = client.post(&url).json(&body);
        if !api_key.is_empty() {
            req = req.bearer_auth(&api_key);
        }

        // ollama 不需要 key
        let res = req.send().await;

        match res {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                if !status.is_success() {
                    return Ok(NodeOutput {
                        status: NodeStatus::Failed,
                        error: Some(format!("AI API 错误 {}: {}", status, &text[..text.len().min(500)])),
                        ..Default::default()
                    });
                }

                let parsed: serde_json::Result<Value> = serde_json::from_str(&text);
                let content = match parsed {
                    Ok(v) => extract_content(&v, &cfg.provider),
                    Err(_) => text.clone(),
                };

                crate::nodes::node::stream_event(&ctx.app, &ctx.execution_id, &ctx.node_id,
                    crate::core::events::StreamKind::Stdout, format!("{}\n", content));

                let _ = body; // suppress unused

                Ok(NodeOutput {
                    value: json!({
                        "content": content,
                        "model": cfg.model,
                        "provider": cfg.provider,
                    }),
                    stdout: content,
                    status: NodeStatus::Success,
                    ..Default::default()
                })
            }
            Err(e) => Ok(NodeOutput::failed(format!("AI 请求失败: {e}"))),
        }
    }
}

fn render(template: &str, ctx: &NodeContext) -> String {
    let mut out = template.to_string();
    // 替换 {{param.name}} (workflow params)
    for (k, v) in &ctx.workflow_params {
        out = out.replace(&format!("{{{{{}}}}}", k), v);
    }
    // 替换 {{nodeId.field}} 简化
    for (id, val) in &ctx.upstream_outputs {
        if let Some(s) = val.get("stdout").and_then(|v| v.as_str()) {
            out = out.replace(&format!("{{{{{}.stdout}}}}", id), s);
        }
        if let Some(s) = val.get("stderr").and_then(|v| v.as_str()) {
            out = out.replace(&format!("{{{{{}.stderr}}}}", id), s);
        }
        if let Some(s) = val.get("output").and_then(|v| v.as_str()) {
            out = out.replace(&format!("{{{{{}.output}}}}", id), s);
        }
    }
    out
}

fn build_messages(system: Option<&str>, user: &str) -> Value {
    let mut msgs = Vec::new();
    if let Some(s) = system {
        msgs.push(json!({ "role": "system", "content": s }));
    }
    msgs.push(json!({ "role": "user", "content": user }));
    json!(msgs)
}

fn extract_content(v: &Value, provider: &str) -> String {
    if provider == "ollama" {
        // ollama /api/chat 返回 message.content
        if let Some(s) = v.pointer("/message/content").and_then(|x| x.as_str()) {
            return s.to_string();
        }
    }
    // OpenAI 兼容
    if let Some(s) = v.pointer("/choices/0/message/content").and_then(|x| x.as_str()) {
        return s.to_string();
    }
    v.to_string()
}

fn read_keyring(name: &str) -> Option<String> {
    let entry = keyring::Entry::new("cmdflow", name).ok()?;
    entry.get_password().ok()
}
