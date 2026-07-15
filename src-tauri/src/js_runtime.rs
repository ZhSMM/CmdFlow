//! JavaScript 插件运行时 (Phase 8)
//!
//! 使用 Boa engine 跑用户写的 JS 脚本。
//! 暴露给 JS 的全局 API:
//! - `cmdflow.log(msg)`: 记录日志
//! - `cmdflow.params`: 当前调用的参数 (只读 object)
//!
//! 用户脚本约定导出一个 `run` 函数, 接收 ctx, 返回 result:
//! ```js
//! function run(ctx) {
//!   cmdflow.log("hello from " + ctx.params.name);
//!   return { ok: true, result: 42 };
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer, property::Attribute,
    Context, JsArgs, JsError, JsResult, JsValue, Source,
};

use crate::error::{AppError, AppResult};

/// 共享的日志收集
#[derive(Debug, Default, Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<Vec<String>>>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&self, msg: String) {
        self.inner.lock().unwrap().push(msg);
    }
    pub fn take(&self) -> Vec<String> {
        let mut l = self.inner.lock().unwrap();
        std::mem::take(&mut *l)
    }
}

/// 加载 JS 源码到 Boa context, 注入 `cmdflow` 全局
pub fn prepare_context(source: &str, logs: LogBuffer) -> AppResult<Context> {
    let mut context = Context::default();

    // 构造 cmdflow.log 函数
    let logs_for_log = logs.clone();
    let log_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, context| {
            let arg = args.get_or_undefined(0);
            let msg = if let Some(s) = arg.as_string() {
                s.to_std_string_escaped()
            } else {
                arg.to_string(context)?.to_std_string_escaped()
            };
            logs_for_log.push(msg);
            Ok(JsValue::Undefined)
        })
    };

    let cmdflow_obj = ObjectInitializer::new(&mut context)
        .function(log_fn, js_string!("log"), 1)
        .build();

    context
        .register_global_property(js_string!("cmdflow"), cmdflow_obj, Attribute::all())
        .map_err(|e| AppError::other(format!("注册全局失败: {e}")))?;

    // 编译并执行用户脚本 (顶层表达式, run 函数定义到 global)
    context
        .eval(Source::from_bytes(source))
        .map_err(|e| AppError::other(format!("JS 编译失败: {e}")))?;

    Ok(context)
}

/// 调用 run(params) 并返回 result
pub fn run(
    context: &mut Context,
    params: HashMap<String, serde_json::Value>,
) -> AppResult<serde_json::Value> {
    let params_json = serde_json::to_string(&params).unwrap_or_else(|_| "{}".to_string());

    // 用 IIFE 调用 run
    let setup = format!(
        r#"
        (function() {{
            const __params = {params_json};
            const __ctx = {{ params: __params }};
            if (typeof run !== 'function') {{
                throw new Error("plugin must export a 'run' function");
            }}
            return run(__ctx);
        }})()
        "#,
        params_json = params_json
    );

    let result = context
        .eval(Source::from_bytes(setup.as_bytes()))
        .map_err(|e| AppError::other(format!("JS 执行失败: {e}")))?;

    js_to_json(context, &result)
}

fn js_to_json(context: &mut Context, value: &JsValue) -> AppResult<serde_json::Value> {
    if value.is_undefined() || value.is_null() {
        return Ok(serde_json::Value::Null);
    }
    if let Some(b) = value.as_boolean() {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Some(n) = value.as_number() {
        return Ok(serde_json::json!(n));
    }
    if let Some(s) = value.as_string() {
        return Ok(serde_json::Value::String(s.to_std_string_escaped()));
    }
    if let Some(obj) = value.as_object() {
        if obj.is_array() {
            let len = obj
                .get(js_string!("length"), context)
                .ok()
                .and_then(|v| v.as_number())
                .unwrap_or(0.0) as usize;
            let mut arr = Vec::with_capacity(len);
            for i in 0..len {
                let v = obj.get(i, context).unwrap_or(JsValue::Undefined);
                arr.push(js_to_json(context, &v)?);
            }
            return Ok(serde_json::Value::Array(arr));
        }
        // 普通 object - 用 own_property_keys 枚举
        let mut map = serde_json::Map::new();
        let keys = obj
            .own_property_keys(context)
            .map_err(|e| AppError::other(format!("JS 枚举属性失败: {e}")))?;
        for k in keys {
            // PropertyKey -> 字符串
            let key_name: String = k.to_string();
            if let Ok(v) = obj.get(k, context) {
                map.insert(key_name, js_to_json(context, &v)?);
            }
        }
        return Ok(serde_json::Value::Object(map));
    }
    Ok(serde_json::Value::Null)
}

/// 加载并执行 (高阶 API, 业务层用)
pub fn execute(
    source: &str,
    params: HashMap<String, serde_json::Value>,
) -> AppResult<PluginExecResult> {
    let logs = LogBuffer::new();
    let mut context = prepare_context(source, logs.clone())?;
    let result = run(&mut context, params)?;
    let log_lines = logs.take();
    Ok(PluginExecResult {
        result,
        logs: log_lines,
    })
}

#[derive(Debug, Clone)]
pub struct PluginExecResult {
    pub result: serde_json::Value,
    pub logs: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_hello() {
        let src = r#"
        function run(ctx) {
            cmdflow.log("hello " + ctx.params.name);
            return { ok: true, name: ctx.params.name };
        }
        "#;
        let mut params = HashMap::new();
        params.insert("name".to_string(), json!("world"));
        let r = execute(src, params).unwrap();
        assert_eq!(r.logs, vec!["hello world".to_string()]);
        assert_eq!(r.result["ok"], json!(true));
        assert_eq!(r.result["name"], json!("world"));
    }

    #[test]
    fn test_math() {
        let src = r#"
        function run(ctx) {
            const a = ctx.params.a;
            const b = ctx.params.b;
            return { sum: a + b, product: a * b };
        }
        "#;
        let mut params = HashMap::new();
        params.insert("a".to_string(), json!(3));
        params.insert("b".to_string(), json!(4));
        let r = execute(src, params).unwrap();
        // Boa 数字全是 f64, 用近似比较
        assert_eq!(r.result["sum"].as_f64(), Some(7.0));
        assert_eq!(r.result["product"].as_f64(), Some(12.0));
    }

    #[test]
    fn test_error() {
        let src = r#"
        function run(ctx) {
            throw new Error("boom");
        }
        "#;
        let r = execute(src, HashMap::new());
        assert!(r.is_err());
    }

    #[test]
    fn test_no_run_function() {
        let src = r#"var x = 1;"#;
        let r = execute(src, HashMap::new());
        assert!(r.is_err());
    }
}
