//! AI 节点 - 调用 LLM (Phase 9.5: 流式输出 + tools/function calling)
//!
//! 支持 OpenAI 兼容 API (/chat/completions, 支持 stream=true)
//! Tools: 工具调用循环,工具名映射到已注册的命令
//!
//! ## 流式输出
//! 通过 `stream: true` 打开 SSE,每个 delta 通过 NodeLog 事件推送,
//! 增量到达,UI 端可以做 typewriter 效果。
//!
//! ## 工具调用循环
//! 1. 把 config.tools (OpenAI 格式) 发给 LLM
//! 2. 若 LLM 返回 tool_calls,在命令库里按工具名查命令
//! 3. 用 tool arguments 作为环境变量 (大写键名) 执行命令
//! 4. 把命令 stdout 作为 tool 响应加回 messages
//! 5. 重新调 LLM,直到 LLM 给出最终回复或达到 max_tool_iterations

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tauri::Manager;

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
    /// 是否用流式 (SSE)。默认 true。
    #[serde(default = "default_stream")]
    stream: bool,
    /// OpenAI 格式工具列表。
    /// 例: [{ "type": "function", "function": { "name": "get_weather", ... } }]
    #[serde(default)]
    tools: Vec<Value>,
    /// tool_choice: "auto" | "none" | "required" | 特定函数
    #[serde(default)]
    tool_choice: Option<Value>,
    /// 工具调用循环最大迭代次数,默认 5
    #[serde(default = "default_max_iter")]
    max_tool_iterations: usize,
}

fn default_temp() -> f32 {
    0.7
}
fn default_stream() -> bool {
    true
}
fn default_max_iter() -> usize {
    5
}

pub struct AiNode;

#[async_trait]
impl Node for AiNode {
    fn type_id(&self) -> &'static str {
        "ai"
    }
    fn display_name(&self) -> &'static str {
        "AI"
    }
    fn category(&self) -> &'static str {
        "io"
    }
    fn description(&self) -> &'static str {
        "调用大模型 (流式 + 工具调用)"
    }

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
                "timeout_ms": { "type": "integer", "title": "超时(毫秒)" },
                "stream": { "type": "boolean", "title": "流式输出 (SSE)", "default": true },
                "tools": {
                    "type": "array",
                    "title": "工具定义 (OpenAI 格式)",
                    "items": { "type": "object" }
                },
                "tool_choice": { "type": "string", "title": "tool_choice (auto/none/required)" },
                "max_tool_iterations": { "type": "integer", "title": "工具调用最大循环次数", "default": 5 }
            }
        })
    }

    async fn execute(
        &self,
        ctx: NodeContext,
        config: Value,
    ) -> crate::error::AppResult<NodeOutput> {
        let cfg: AiConfig = serde_json::from_value(config)
            .map_err(|e| crate::error::AppError::invalid(format!("ai config 解析失败: {e}")))?;

        let prompt = render(&cfg.prompt, &ctx);

        let api_key = if let Some(key_ref) = &cfg.api_key_ref {
            read_keyring(key_ref).unwrap_or_default()
        } else {
            std::env::var("OPENAI_API_KEY").unwrap_or_default()
        };

        let base_url = cfg
            .base_url
            .clone()
            .unwrap_or_else(|| match cfg.provider.as_str() {
                "openai" => "https://api.openai.com/v1".to_string(),
                "ollama" => "http://localhost:11434/v1".to_string(),
                "anthropic" => "https://api.anthropic.com/v1".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            });

        let client = Client::builder()
            .timeout(Duration::from_millis(cfg.timeout_ms.unwrap_or(120_000)))
            .build()
            .map_err(|e| crate::error::AppError::other(format!("http client: {e}")))?;

        crate::nodes::node::stream_event(
            &ctx.app,
            &ctx.execution_id,
            &ctx.node_id,
            crate::core::events::StreamKind::System,
            format!(
                "🤖 调用 {}/{} (stream={}, tools={})\n",
                cfg.provider,
                cfg.model,
                cfg.stream,
                cfg.tools.len()
            ),
        );

        // messages 数组 (支持工具调用循环,会原地追加)
        let mut messages = build_messages(cfg.system.as_deref(), &prompt);

        // 1. 第一次请求
        let mut full_content = String::new();
        let mut tool_calls = match make_request(
            &ctx,
            &client,
            &base_url,
            &api_key,
            &cfg,
            &messages,
            &mut full_content,
        )
        .await
        {
            Ok(t) => t,
            Err(out) => return Ok(out),
        };

        // 2. 工具调用循环
        let mut iter = 0;
        while !tool_calls.is_empty() && iter < cfg.max_tool_iterations {
            iter += 1;
            crate::nodes::node::stream_event(
                &ctx.app,
                &ctx.execution_id,
                &ctx.node_id,
                crate::core::events::StreamKind::System,
                format!("🔧 第 {} 轮工具调用 ({} 个)\n", iter, tool_calls.len()),
            );

            // 把 assistant 的 tool_calls 消息加回 messages
            let assistant_msg = json!({
                "role": "assistant",
                "content": if full_content.is_empty() { Value::Null } else { Value::String(full_content.clone()) },
                "tool_calls": tool_calls.iter().map(|tc| json!({
                    "id": tc.id,
                    "type": tc.call_type,
                    "function": {
                        "name": tc.function_name,
                        "arguments": tc.function_args,
                    }
                })).collect::<Vec<_>>()
            });
            messages.as_array_mut().unwrap().push(assistant_msg);

            // 逐个执行工具
            for tc in &tool_calls {
                let tool_name = &tc.function_name;
                let args_str = &tc.function_args;
                let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                crate::nodes::node::stream_event(
                    &ctx.app,
                    &ctx.execution_id,
                    &ctx.node_id,
                    crate::core::events::StreamKind::System,
                    format!("  → {}({})\n", tool_name, args_str),
                );

                // 在命令库中查 tool_name 并执行
                let tool_result = match lookup_and_run_command(&ctx, tool_name, &args).await {
                    Ok(s) => s,
                    Err(e) => {
                        crate::nodes::node::stream_event(
                            &ctx.app,
                            &ctx.execution_id,
                            &ctx.node_id,
                            crate::core::events::StreamKind::Stderr,
                            format!("工具执行失败: {e}\n"),
                        );
                        format!("error: {e}")
                    }
                };

                // 把工具结果加回 messages
                messages.as_array_mut().unwrap().push(json!({
                    "role": "tool",
                    "tool_call_id": tc.id,
                    "content": tool_result,
                }));
            }

            // 再发请求 (后续请求统一用非流式,简化逻辑)
            let follow_up = match follow_up_request(
                &ctx, &client, &base_url, &api_key, &cfg, &messages,
            )
            .await
            {
                Ok(r) => r,
                Err(out) => return Ok(out),
            };
            full_content.push_str(&follow_up.content);
            if !follow_up.content.is_empty() {
                crate::nodes::node::stream_event(
                    &ctx.app,
                    &ctx.execution_id,
                    &ctx.node_id,
                    crate::core::events::StreamKind::Stdout,
                    follow_up.content.clone(),
                );
            }
            tool_calls = follow_up.tool_calls;
        }

        Ok(NodeOutput {
            value: json!({
                "content": full_content,
                "model": cfg.model,
                "provider": cfg.provider,
            }),
            stdout: full_content,
            status: NodeStatus::Success,
            ..Default::default()
        })
    }
}

#[derive(Default, Debug, Clone)]
struct ToolCall {
    id: String,
    call_type: String,
    function_name: String,
    function_args: String,
}

/// 在命令库中按名称查命令并执行 (工具调用)
async fn lookup_and_run_command(
    ctx: &NodeContext,
    tool_name: &str,
    args: &Value,
) -> Result<String, String> {
    use rusqlite::OptionalExtension;

    let conn = ctx.db.get().map_err(|e| format!("DB: {e}"))?;

    // 按名称查,只要 id
    let command_id: Option<String> = conn
        .query_row(
            "SELECT id FROM commands WHERE name = ?1 LIMIT 1",
            [tool_name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("查命令失败: {e}"))?;

    let command_id = command_id.ok_or_else(|| format!("未找到命令: {tool_name}"))?;

    // 把 args 转为 params (大写键名,值为 JSON Value)
    let mut env = std::collections::HashMap::new();
    if let Some(obj) = args.as_object() {
        for (k, v) in obj {
            let key = k.to_uppercase();
            env.insert(key, v.clone());
        }
    }

    // 调 execution::run_command_inner 跑命令
    let input = crate::commands::execution::RunCommandInput {
        command_id: command_id.clone(),
        params: env,
        override_safety: false,
    };
    // 从 app_handle 拿 ExecutionRegistry
    let app_state = ctx.app.state::<crate::state::AppState>();
    let registry = app_state.execution_registry.clone();
    match crate::commands::execution::run_command_inner(
        ctx.app.clone(),
        ctx.db.clone(),
        registry,
        input,
    )
    .await
    {
        Ok(resp) => Ok(resp.stdout),
        Err(e) => Err(format!("命令执行失败: {e}")),
    }
}

fn render(template: &str, ctx: &NodeContext) -> String {
    let mut out = template.to_string();
    for (k, v) in &ctx.workflow_params {
        out = out.replace(&format!("{{{{{}}}}}", k), v);
    }
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

fn extract_content(v: &Value, _provider: &str) -> Option<String> {
    if let Some(s) = v
        .pointer("/choices/0/message/content")
        .and_then(|x| x.as_str())
    {
        return Some(s.to_string());
    }
    if let Some(s) = v.pointer("/message/content").and_then(|x| x.as_str()) {
        return Some(s.to_string());
    }
    None
}

fn extract_tool_calls(v: &Value) -> Vec<ToolCall> {
    let Some(arr) = v
        .pointer("/choices/0/message/tool_calls")
        .and_then(|x| x.as_array())
    else {
        return vec![];
    };
    arr.iter()
        .map(|tc| ToolCall {
            id: tc
                .get("id")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string(),
            call_type: tc
                .get("type")
                .and_then(|s| s.as_str())
                .unwrap_or("function")
                .to_string(),
            function_name: tc
                .pointer("/function/name")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string(),
            function_args: tc
                .pointer("/function/arguments")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect()
}

struct FollowUpResult {
    content: String,
    tool_calls: Vec<ToolCall>,
}

/// 第一次请求:支持流式 / 非流式,返回 tool_calls
async fn make_request(
    ctx: &NodeContext,
    client: &Client,
    base_url: &str,
    api_key: &str,
    cfg: &AiConfig,
    messages: &Value,
    full_content: &mut String,
) -> Result<Vec<ToolCall>, NodeOutput> {
    let mut body = json!({
        "model": cfg.model,
        "messages": messages,
        "temperature": cfg.temperature,
    });
    if cfg.stream {
        body["stream"] = json!(true);
    }
    if !cfg.tools.is_empty() {
        body["tools"] = json!(cfg.tools);
        if let Some(tc) = &cfg.tool_choice {
            body["tool_choice"] = tc.clone();
        }
    }

    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut req = client.post(&url).json(&body);
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| NodeOutput::failed(format!("AI 请求失败: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(NodeOutput {
            status: NodeStatus::Failed,
            error: Some(format!(
                "AI API 错误 {}: {}",
                status,
                &text[..text.len().min(500)]
            )),
            ..Default::default()
        });
    }

    if cfg.stream {
        // 流式: 逐 chunk 读 SSE
        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut finish_reason: Option<String> = None;
        let mut acc_tool_calls: Vec<ToolCall> = Vec::new();

        while let Some(chunk) = stream.next().await {
            if ctx.cancel.is_cancelled() {
                return Err(NodeOutput {
                    status: NodeStatus::Cancelled,
                    error: Some("用户取消".into()),
                    ..Default::default()
                });
            }
            let bytes = chunk.map_err(|e| NodeOutput::failed(format!("SSE 读失败: {e}")))?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            // 解析 SSE 事件
            while let Some(idx) = buf.find("\n\n") {
                let event = buf[..idx].to_string();
                buf = buf[idx + 2..].to_string();
                for line in event.lines() {
                    let line = line.trim_start();
                    if let Some(data) = line.strip_prefix("data: ") {
                        let data = data.trim();
                        if data == "[DONE]" {
                            continue;
                        }
                        if let Ok(v) = serde_json::from_str::<Value>(data) {
                            if let Some(delta) = v.pointer("/choices/0/delta") {
                                if let Some(content) = delta.get("content").and_then(|c| c.as_str())
                                {
                                    if !content.is_empty() {
                                        full_content.push_str(content);
                                        crate::nodes::node::stream_event(
                                            &ctx.app,
                                            &ctx.execution_id,
                                            &ctx.node_id,
                                            crate::core::events::StreamKind::Stdout,
                                            content.to_string(),
                                        );
                                    }
                                }
                                if let Some(tcs) =
                                    delta.get("tool_calls").and_then(|t| t.as_array())
                                {
                                    for tc in tcs {
                                        let idx =
                                            tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0)
                                                as usize;
                                        while acc_tool_calls.len() <= idx {
                                            acc_tool_calls.push(ToolCall::default());
                                        }
                                        if let Some(id) = tc.get("id").and_then(|s| s.as_str()) {
                                            acc_tool_calls[idx].id = id.to_string();
                                        }
                                        if let Some(t) = tc.get("type").and_then(|s| s.as_str()) {
                                            acc_tool_calls[idx].call_type = t.to_string();
                                        }
                                        if let Some(func) = tc.get("function") {
                                            if let Some(name) =
                                                func.get("name").and_then(|s| s.as_str())
                                            {
                                                acc_tool_calls[idx].function_name.push_str(name);
                                            }
                                            if let Some(args) =
                                                func.get("arguments").and_then(|s| s.as_str())
                                            {
                                                acc_tool_calls[idx].function_args.push_str(args);
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(fr) = v
                                .pointer("/choices/0/finish_reason")
                                .and_then(|x| x.as_str())
                            {
                                finish_reason = Some(fr.to_string());
                            }
                        }
                    }
                }
            }
        }

        if finish_reason.as_deref() == Some("tool_calls") && !acc_tool_calls.is_empty() {
            Ok(acc_tool_calls)
        } else {
            Ok(vec![])
        }
    } else {
        // 非流式
        let text = resp
            .text()
            .await
            .map_err(|e| NodeOutput::failed(format!("读 AI 响应失败: {e}")))?;
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| NodeOutput::failed(format!("AI 响应不是 JSON: {e}")))?;
        if let Some(content) = extract_content(&v, &cfg.provider) {
            full_content.push_str(&content);
            crate::nodes::node::stream_event(
                &ctx.app,
                &ctx.execution_id,
                &ctx.node_id,
                crate::core::events::StreamKind::Stdout,
                content,
            );
        }
        Ok(extract_tool_calls(&v))
    }
}

/// 工具调用后的回呼请求:非流式
async fn follow_up_request(
    ctx: &NodeContext,
    client: &Client,
    base_url: &str,
    api_key: &str,
    cfg: &AiConfig,
    messages: &Value,
) -> Result<FollowUpResult, NodeOutput> {
    let body = json!({
        "model": cfg.model,
        "messages": messages,
        "temperature": cfg.temperature,
    });
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut req = client.post(&url).json(&body);
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| NodeOutput::failed(format!("AI 工具回呼失败: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(NodeOutput {
            status: NodeStatus::Failed,
            error: Some(format!(
                "AI 工具回呼错误 {}: {}",
                status,
                &text[..text.len().min(500)]
            )),
            ..Default::default()
        });
    }
    let text = resp
        .text()
        .await
        .map_err(|e| NodeOutput::failed(format!("读工具回呼响应失败: {e}")))?;
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| NodeOutput::failed(format!("工具回呼不是 JSON: {e}")))?;
    let content = extract_content(&v, &cfg.provider).unwrap_or_default();
    let tool_calls = extract_tool_calls(&v);

    if !content.is_empty() {
        crate::nodes::node::stream_event(
            &ctx.app,
            &ctx.execution_id,
            &ctx.node_id,
            crate::core::events::StreamKind::Stdout,
            content.clone(),
        );
    }
    let _ = ctx; // avoid unused warning
    Ok(FollowUpResult {
        content,
        tool_calls,
    })
}

fn read_keyring(name: &str) -> Option<String> {
    let entry = keyring::Entry::new("cmdflow", name).ok()?;
    entry.get_password().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_content_openai_format() {
        let v = json!({
            "choices": [{
                "message": { "role": "assistant", "content": "Hello, world!" }
            }]
        });
        assert_eq!(
            extract_content(&v, "openai"),
            Some("Hello, world!".to_string())
        );
    }

    #[test]
    fn extract_content_ollama_format() {
        let v = json!({
            "message": { "role": "assistant", "content": "Hi from ollama" }
        });
        assert_eq!(
            extract_content(&v, "ollama"),
            Some("Hi from ollama".to_string())
        );
    }

    #[test]
    fn extract_content_none_when_missing() {
        let v = json!({ "choices": [] });
        assert_eq!(extract_content(&v, "openai"), None);
    }

    #[test]
    fn extract_tool_calls_basic() {
        let v = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [
                        {
                            "id": "call_123",
                            "type": "function",
                            "function": {
                                "name": "get_weather",
                                "arguments": "{\"city\": \"Beijing\"}"
                            }
                        }
                    ]
                }
            }]
        });
        let calls = extract_tool_calls(&v);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_123");
        assert_eq!(calls[0].function_name, "get_weather");
        assert_eq!(calls[0].function_args, "{\"city\": \"Beijing\"}");
    }

    #[test]
    fn extract_tool_calls_empty() {
        let v = json!({ "choices": [{ "message": { "content": "no tools" } }] });
        assert!(extract_tool_calls(&v).is_empty());
    }

    #[test]
    fn build_messages_with_and_without_system() {
        let with_sys = build_messages(Some("you are helpful"), "hi");
        let arr = with_sys.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[1]["role"], "user");

        let no_sys = build_messages(None, "hi");
        assert_eq!(no_sys.as_array().unwrap().len(), 1);
    }
}
