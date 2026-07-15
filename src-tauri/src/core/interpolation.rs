//! 变量插值
//!
//! 语法: `{{name}}` 或 `{{prefix.path}}`
//!
//! 支持的 prefix:
//! - `name`              命令/节点参数
//! - `nodeId.field`      节点输出字段（Phase 2 用）
//! - `nodeId.stdout`     节点 stdout 全文
//! - `nodeId.exit_code`  节点退出码
//! - `env.VAR_NAME`      环境变量
//! - `now`               当前时间 ISO 8601
//! - `$secret.name`      keyring 加密 secret
//!
//! 简单实现:正则解析 + 字符串替换。
//! 不使用 handlebars/liquid 等模板引擎，避免引入额外依赖。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

use crate::error::{AppError, AppResult};

/// 插值上下文
#[derive(Debug, Default, Clone)]
pub struct InterpContext {
    /// 命令/节点参数: name -> value
    pub params: HashMap<String, String>,
    /// 环境变量
    pub env: HashMap<String, String>,
    /// 节点输出: nodeId -> { field -> value }
    pub node_outputs: HashMap<String, Value>,
    /// keyring secrets: name -> value
    pub secrets: HashMap<String, String>,
}

impl InterpContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(name.into(), value.into());
        self
    }

    pub fn with_env(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(name.into(), value.into());
        self
    }

    /// 解析单个值
    pub fn resolve(&self, key: &str) -> Option<String> {
        // 1. secrets
        if let Some(name) = key.strip_prefix("$secret.") {
            return self.secrets.get(name).cloned();
        }

        // 2. now
        if key == "now" {
            return Some(Utc::now().to_rfc3339());
        }

        // 3. env.VAR
        if let Some(name) = key.strip_prefix("env.") {
            if let Some(v) = self.env.get(name) {
                return Some(v.clone());
            }
            // 退化:从系统环境读
            return std::env::var(name).ok();
        }

        // 4. nodeId.field / nodeId.stdout / nodeId.exit_code
        if let Some((node_id, field)) = split_once(key, '.') {
            if let Some(node) = self.node_outputs.get(node_id) {
                if field == "stdout" || field == "stderr" {
                    if let Some(s) = node.get(field).and_then(|v| v.as_str()) {
                        return Some(s.to_string());
                    }
                }
                if let Some(v) = node.get(field) {
                    return Some(json_to_string(v));
                }
            }
        }

        // 5. 参数
        if let Some(v) = self.params.get(key) {
            return Some(v.clone());
        }

        None
    }
}

fn split_once(s: &str, sep: char) -> Option<(&str, &str)> {
    let idx = s.find(sep)?;
    Some((&s[..idx], &s[idx + sep.len_utf8()..]))
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

/// 模板字符串中的插值
pub fn render(template: &str, ctx: &InterpContext) -> AppResult<String> {
    let re = PATTERN.get_or_init(|| {
        // 匹配 {{ ... }}，非贪婪，允许空白
        Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").unwrap()
    });

    let mut result = String::with_capacity(template.len());
    let mut last_end = 0;

    for caps in re.captures_iter(template) {
        let m = caps.get(0).unwrap();
        let key = caps.get(1).unwrap().as_str().trim();

        // 把匹配之前的原文 push 进去
        result.push_str(&template[last_end..m.start()]);

        match ctx.resolve(key) {
            Some(val) => result.push_str(&val),
            None => {
                return Err(AppError::invalid(format!("变量未定义: {}", key)));
            }
        }

        last_end = m.end();
    }
    result.push_str(&template[last_end..]);
    Ok(result)
}

static PATTERN: OnceLock<Regex> = OnceLock::new();

/// 工具:当前时间戳 (秒)
pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 工具:把 DateTime 转为 unix 时间戳
pub fn dt_to_ts(dt: DateTime<Utc>) -> i64 {
    dt.timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> InterpContext {
        InterpContext::new()
            .with_param("name", "world")
            .with_env("USER", "alice")
    }

    #[test]
    fn test_simple_param() {
        let out = render("hello {{name}}", &ctx()).unwrap();
        assert_eq!(out, "hello world");
    }

    #[test]
    fn test_env() {
        let out = render("user={{env.USER}}", &ctx()).unwrap();
        assert_eq!(out, "user=alice");
    }

    #[test]
    fn test_now() {
        let out = render("{{now}}", &ctx()).unwrap();
        assert!(!out.is_empty());
    }

    #[test]
    fn test_missing_var_errors() {
        let r = render("{{nope}}", &ctx());
        assert!(r.is_err());
    }

    #[test]
    fn test_no_placeholder() {
        let out = render("just text", &ctx()).unwrap();
        assert_eq!(out, "just text");
    }

    #[test]
    fn test_multiple_placeholders() {
        let out = render("{{name}} {{name}}", &ctx()).unwrap();
        assert_eq!(out, "world world");
    }
}
