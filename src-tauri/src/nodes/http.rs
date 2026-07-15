//! HTTP 节点

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

use super::node::{render_config, Node, NodeContext, NodeOutput};
use crate::core::events::NodeStatus;
use crate::core::interpolation::InterpContext;

#[derive(Debug, Deserialize)]
struct HttpConfig {
    method: String,
    url: String,
    #[serde(default)]
    headers: HashMap<String, String>,
    #[serde(default)]
    body: Value,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

pub struct HttpNode;

#[async_trait]
impl Node for HttpNode {
    fn type_id(&self) -> &'static str {
        "http"
    }
    fn display_name(&self) -> &'static str {
        "HTTP"
    }
    fn category(&self) -> &'static str {
        "io"
    }
    fn description(&self) -> &'static str {
        "发送 HTTP 请求"
    }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["method", "url"],
            "properties": {
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD"],
                    "title": "方法"
                },
                "url": { "type": "string", "title": "URL" },
                "headers": { "type": "object", "title": "请求头" },
                "body": { "type": "object", "title": "请求体" },
                "timeout_ms": { "type": "integer", "title": "超时(毫秒)" }
            }
        })
    }

    async fn execute(
        &self,
        ctx: NodeContext,
        config: Value,
    ) -> crate::error::AppResult<NodeOutput> {
        let cfg: HttpConfig = serde_json::from_value(config.clone())
            .map_err(|e| crate::error::AppError::invalid(format!("http config 解析失败: {e}")))?;

        // 渲染 url + headers
        let mut interp = InterpContext::new();
        for (k, v) in &ctx.workflow_params {
            interp.params.insert(k.clone(), v.clone());
        }
        let url = render_config(&Value::String(cfg.url.clone()), &interp)?
            .as_str()
            .map(String::from)
            .unwrap_or_default();

        let mut headers = HashMap::new();
        for (k, v) in &cfg.headers {
            let rv = render_config(&Value::String(v.clone()), &interp)?
                .as_str()
                .map(String::from)
                .unwrap_or_default();
            headers.insert(k.clone(), rv);
        }

        let body = render_config(&cfg.body, &interp)?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(cfg.timeout_ms.unwrap_or(30000)))
            .build()
            .map_err(|e| crate::error::AppError::other(format!("构建 http client 失败: {e}")))?;

        let mut req = client.request(
            cfg.method.parse().map_err(|_| {
                crate::error::AppError::invalid(format!("无效方法: {}", cfg.method))
            })?,
            &url,
        );

        for (k, v) in &headers {
            req = req.header(k, v);
        }

        // 发送 body
        if !body.is_null() {
            if body.is_string() {
                if let Some(s) = body.as_str() {
                    req = req.body(s.to_string());
                }
            } else {
                req = req.json(&body);
            }
        }

        crate::nodes::node::stream_event(
            &ctx.app,
            &ctx.execution_id,
            &ctx.node_id,
            crate::core::events::StreamKind::System,
            format!("→ {} {}\n", cfg.method, url),
        );

        let res = req.send().await;

        match res {
            Ok(resp) => {
                let status = resp.status();
                let status_code = status.as_u16();
                let resp_headers: HashMap<String, String> = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body_text = resp.text().await.unwrap_or_default();

                crate::nodes::node::stream_event(
                    &ctx.app,
                    &ctx.execution_id,
                    &ctx.node_id,
                    crate::core::events::StreamKind::System,
                    format!("← {} ({} bytes)\n", status, body_text.len()),
                );

                let ok = status.is_success();
                Ok(NodeOutput {
                    value: json!({
                        "status": status_code,
                        "ok": ok,
                        "headers": resp_headers,
                        "body": body_text,
                    }),
                    stdout: body_text,
                    status: if ok {
                        NodeStatus::Success
                    } else {
                        NodeStatus::Failed
                    },
                    error: if !ok {
                        Some(format!("HTTP {}", status_code))
                    } else {
                        None
                    },
                    ..Default::default()
                })
            }
            Err(e) => Ok(NodeOutput::failed(format!("HTTP 请求失败: {e}"))),
        }
    }
}
