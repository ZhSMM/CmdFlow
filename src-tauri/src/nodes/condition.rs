//! 条件节点 - 表达式求值，返回 "true" 或 "false" 分支

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::node::{Node, NodeContext, NodeOutput};
use crate::core::events::NodeStatus;

#[derive(Debug, Deserialize)]
struct ConditionConfig {
    /// 表达式,支持 ==, !=, >, <, >=, <=, &&, ||, !, 字面量
    expression: String,
    #[serde(default = "default_true")]
    true_label: String,
    #[serde(default = "default_false")]
    false_label: String,
}

fn default_true() -> String { "true".to_string() }
fn default_false() -> String { "false".to_string() }

pub struct ConditionNode;

#[async_trait]
impl Node for ConditionNode {
    fn type_id(&self) -> &'static str { "condition" }
    fn display_name(&self) -> &'static str { "条件" }
    fn category(&self) -> &'static str { "control" }
    fn description(&self) -> &'static str { "条件分支" }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["expression"],
            "properties": {
                "expression": { "type": "string", "title": "表达式", "format": "textarea" },
                "true_label": { "type": "string", "title": "true 分支标签" },
                "false_label": { "type": "string", "title": "false 分支标签" }
            }
        })
    }

    async fn execute(&self, _ctx: NodeContext, config: Value) -> crate::error::AppResult<NodeOutput> {
        let cfg: ConditionConfig = serde_json::from_value(config)
            .map_err(|e| crate::error::AppError::invalid(format!("condition config 解析失败: {e}")))?;

        // Phase 2 简化: 表达式必须返回 "true" / "false"
        // 简单实现: 用 boolector 风格的条件求值
        // 支持格式: "nodeId.field == 'literal'" 或 "value > 0" 等
        // 完整表达式引擎留 Phase 4
        let result = evaluate_simple(&cfg.expression, &_ctx);

        let branch = if result { cfg.true_label.clone() } else { cfg.false_label.clone() };

        Ok(NodeOutput {
            value: json!({
                "result": result,
                "branch": branch,
                "expression": cfg.expression,
            }),
            branches: vec![branch],
            status: NodeStatus::Success,
            ..Default::default()
        })
    }
}

/// 简化表达式求值:
/// - "key" -> bool
/// - "key == 'literal'" / "key != 1" / "key > 0" / "key < 10"
/// - 支持 && ||
/// - 变量来自 ctx.upstream_outputs (JSON value)
fn evaluate_simple(expr: &str, ctx: &NodeContext) -> bool {
    let expr = expr.trim();

    // 简单的 && / || 切分
    if let Some(idx) = find_top_level(expr, "||") {
        let (l, r) = expr.split_at(idx);
        let r = &r[2..];
        return evaluate_simple(l.trim(), ctx) || evaluate_simple(r.trim(), ctx);
    }
    if let Some(idx) = find_top_level(expr, "&&") {
        let (l, r) = expr.split_at(idx);
        let r = &r[2..];
        return evaluate_simple(l.trim(), ctx) && evaluate_simple(r.trim(), ctx);
    }

    // 单条件: "LHS OP RHS"
    for op in &["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(idx) = expr.find(op) {
            let lhs = expr[..idx].trim();
            let rhs = expr[idx + op.len()..].trim();
            let lv = lookup_value(lhs, ctx);
            let rv = parse_literal(rhs);
            return match *op {
                "==" => json_eq(&lv, &rv),
                "!=" => !json_eq(&lv, &rv),
                ">" => json_cmp(&lv, &rv).map(|o| o == std::cmp::Ordering::Greater).unwrap_or(false),
                "<" => json_cmp(&lv, &rv).map(|o| o == std::cmp::Ordering::Less).unwrap_or(false),
                ">=" => json_cmp(&lv, &rv).map(|o| o != std::cmp::Ordering::Less).unwrap_or(false),
                "<=" => json_cmp(&lv, &rv).map(|o| o != std::cmp::Ordering::Greater).unwrap_or(false),
                _ => false,
            };
        }
    }

    // 单变量: 真假取决于存在 + 非空 + 非 false/0
    let v = lookup_value(expr, ctx);
    is_truthy(&v)
}

fn find_top_level(s: &str, op: &str) -> Option<usize> {
    let mut depth = 0i32;
    let bytes = s.as_bytes();
    let op_bytes = op.as_bytes();
    let mut i = 0;
    while i + op_bytes.len() <= bytes.len() {
        let c = bytes[i] as char;
        if c == '(' { depth += 1; }
        else if c == ')' { depth -= 1; }
        else if depth == 0 && &bytes[i..i+op_bytes.len()] == op_bytes {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn lookup_value(path: &str, ctx: &NodeContext) -> Value {
    let path = path.trim().trim_matches(|c| c == '(' || c == ')');
    // nodeId.field
    if let Some(dot) = path.find('.') {
        let node_id = &path[..dot];
        let field = &path[dot+1..];
        if let Some(node) = ctx.upstream_outputs.get(node_id) {
            if let Some(v) = node.get(field) {
                return v.clone();
            }
        }
        return Value::Null;
    }
    // 单 key, 在 workflow_params 中找
    if let Some(v) = ctx.workflow_params.get(path) {
        return Value::String(v.clone());
    }
    // 在所有 upstream_outputs 中浅查
    for (_id, output) in &ctx.upstream_outputs {
        if let Some(v) = output.get(path) {
            return v.clone();
        }
    }
    Value::Null
}

fn parse_literal(s: &str) -> Value {
    let s = s.trim();
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        return Value::String(s[1..s.len()-1].to_string());
    }
    if s == "true" { return Value::Bool(true); }
    if s == "false" { return Value::Bool(false); }
    if s == "null" { return Value::Null; }
    if let Ok(n) = s.parse::<i64>() { return Value::Number(n.into()); }
    if let Ok(f) = s.parse::<f64>() { return serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null); }
    Value::String(s.to_string())
}

fn json_eq(a: &Value, b: &Value) -> bool {
    a == b
}

fn json_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let xf = x.as_f64()?;
            let yf = y.as_f64()?;
            xf.partial_cmp(&yf)
        }
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(s) => !s.is_empty() && s != "false" && s != "0",
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}
