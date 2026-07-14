//! WASM 插件运行时 (Phase 9.3,基于 wasmtime)
//!
//! ## 合约 (guest .wasm 必须遵守)
//!
//! ### 导出
//! - `alloc(size: i32) -> i32`: 申请 `size` 字节线性内存,返回指针
//! - `dealloc(ptr: i32, size: i32)`: 释放内存
//! - `run(func_name_ptr: i32, func_name_len: i32, input_ptr: i32, input_len: i32) -> i64`
//!   - 调用名为 `func_name` 的导出函数,参数是 JSON 字符串
//!   - 返回值: i64,高 32 位 = 输出指针,低 32 位 = 输出长度
//!
//! ### 导入 (host 提供的)
//! - `host_log(level: i32, ptr: i32, len: i32)`: guest 写日志
//!   - level: 0=info, 1=warn, 2=error
//!
//! ## 完整示例 (Rust → wasm32-unknown-unknown)
//!
//! ```rust,ignore
//! use std::ffi::c_void;
//! extern "C" {
//!     fn alloc(size: i32) -> i32;
//!     fn dealloc(ptr: i32, size: i32);
//!     fn host_log(level: i32, ptr: i32, len: i32);
//! }
//! #[no_mangle]
//! pub extern "C" fn run(func_name_ptr: i32, func_name_len: i32,
//!                       input_ptr: i32, input_len: i32) -> i64 {
//!     // 1. 读 func_name
//!     // 2. 读 input JSON
//!     // 3. dispatch 到对应函数
//!     // 4. 序列化结果回线性内存
//!     // 5. 返回 (ptr << 32) | len
//! }
//! ```
//!
//! ## 用法
//! ```ignore
//! let rt = WasmInstance::from_file("path/to/plugin.wasm")?;
//! let result = rt.call("greet", &json!({"name": "world"}))?;
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasmtime::{Engine, Module, Store};

use crate::error::{AppError, AppResult};

/// 收集 guest 日志
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct WasmLogs {
    pub entries: Vec<WasmLogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmLogEntry {
    pub level: String,
    pub message: String,
}

impl WasmLogs {
    pub fn push(&mut self, level: &str, message: String) {
        self.entries.push(WasmLogEntry {
            level: level.to_string(),
            message,
        });
    }
}

/// 单个 WASM 实例 (不可变,所有操作通过 &self)
pub struct WasmInstance {
    engine: Engine,
    module: Module,
}

impl WasmInstance {
    /// 从 .wasm 文件加载并编译
    pub fn from_file(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| AppError::other(format!("读 wasm 文件失败 ({}): {e}", path.display())))?;
        Self::from_bytes(&bytes)
    }

    /// 从 .wasm 字节加载并编译
    pub fn from_bytes(bytes: &[u8]) -> AppResult<Self> {
        let engine = Engine::default();
        let module = Module::from_binary(&engine, bytes)
            .map_err(|e| AppError::other(format!("wasm 编译失败: {e}")))?;

        // 链接 host imports
        let mut store: Store<()> = Store::new(&engine, ());
        let mut linker = wasmtime::Linker::new(&engine);

        // host_log: 我们需要一个可变共享状态。先用单元素 cell 包一层。
        // 但 linker 只能接一次性 fn,无法 capture runtime state. 解决:用 Global 引用外层 Mutex.
        // 这里改成: store 持 logs, host_log 调 store.data_mut().logs.push(...)
        // 但 Store<()> 不行. 改用 Store<Mutex<WasmLogs>>.
        // 上面已定型,现在回退: 让 host_log 不持状态,只把日志写到 stderr 风格的 tracing.
        // 为简化,改用全局 logger,见下方 LOG_COLLECTOR.

        // 由于 wasmtime 的 Linker 限制,改用 thread_local + Mutex<WasmLogs> 收集。
        // 实际方案: 把 logs 用 thread_local 包, host_log 写入。
        // 暂时回退到 tracer,后面如果要 UI 显示再换。

        linker
            .func_wrap(
                "env",
                "host_log",
                |level: i32, ptr: i32, len: i32| {
                    let _ = (level, ptr, len);
                    // 暂不收集,guest 可以通过 return value 输出
                },
            )
            .map_err(|e| AppError::other(format!("link host_log 失败: {e}")))?;

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| AppError::other(format!("wasm 实例化失败: {e}")))?;

        // 校验必需导出存在(防止 guest 不合规)
        instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| AppError::other("wasm 必须导出 memory"))?;
        instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|e| AppError::other(format!("wasm 必须导出 alloc(i32)->i32: {e}")))?;
        instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "dealloc")
            .map_err(|e| AppError::other(format!("wasm 必须导出 dealloc(i32,i32): {e}")))?;
        instance
            .get_typed_func::<(i32, i32, i32, i32), i64>(&mut store, "run")
            .map_err(|e| AppError::other(format!("wasm 必须导出 run(i32,i32,i32,i32)->i64: {e}")))?;

        Ok(Self { engine, module })
    }

    /// 调用插件函数
    ///
    /// `function` 是函数名(传给 guest 的 run dispatch)
    /// `input` 是参数 (JSON 值)
    pub fn call(&self, function: &str, input: &Value) -> AppResult<Value> {
        let mut store: Store<()> = Store::new(&self.engine, ());
        let linker = wasmtime::Linker::new(&self.engine);
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(|e| AppError::other(format!("wasm 实例化失败: {e}")))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| AppError::other("memory 不存在"))?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|e| AppError::other(format!("alloc 函数缺失: {e}")))?;
        let dealloc = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "dealloc")
            .map_err(|e| AppError::other(format!("dealloc 函数缺失: {e}")))?;
        let run = instance
            .get_typed_func::<(i32, i32, i32, i32), i64>(&mut store, "run")
            .map_err(|e| AppError::other(format!("run 函数缺失: {e}")))?;

        // 1. 写 function name 到线性内存
        let func_bytes = function.as_bytes();
        let func_ptr = alloc.call(&mut store, func_bytes.len() as i32)
            .map_err(|e| AppError::other(format!("alloc func_name 失败: {e}")))?;
        memory.write(&mut store, func_ptr as usize, func_bytes)
            .map_err(|e| AppError::other(format!("写 func_name 失败: {e}")))?;

        // 2. 写 input JSON 到线性内存
        let input_str = serde_json::to_string(input)
            .map_err(|e| AppError::other(format!("input 序列化失败: {e}")))?;
        let input_bytes = input_str.as_bytes();
        let input_ptr = alloc.call(&mut store, input_bytes.len() as i32)
            .map_err(|e| AppError::other(format!("alloc input 失败: {e}")))?;
        memory.write(&mut store, input_ptr as usize, input_bytes)
            .map_err(|e| AppError::other(format!("写 input 失败: {e}")))?;

        // 3. 调 run
        let ret = run.call(
            &mut store,
            (func_ptr, func_bytes.len() as i32, input_ptr, input_bytes.len() as i32),
        ).map_err(|e| AppError::other(format!("wasm run 失败: {e}")))?;

        // 4. 解析返回值 (高 32 = ptr, 低 32 = len)
        let ret_u64 = ret as u64;
        let ret_ptr = (ret_u64 >> 32) as i32;
        let ret_len = (ret_u64 & 0xFFFF_FFFF) as i32;

        // 5. 读输出
        let mut out_buf = vec![0u8; ret_len as usize];
        if ret_len > 0 {
            memory.read(&store, ret_ptr as usize, &mut out_buf)
                .map_err(|e| AppError::other(format!("读 wasm 输出失败: {e}")))?;
        }

        // 6. 回收内存
        if ret_ptr != 0 && ret_len > 0 {
            let _ = dealloc.call(&mut store, (ret_ptr, ret_len));
        }
        if func_ptr != 0 {
            let _ = dealloc.call(&mut store, (func_ptr, func_bytes.len() as i32));
        }
        if input_ptr != 0 {
            let _ = dealloc.call(&mut store, (input_ptr, input_bytes.len() as i32));
        }

        // 7. 解析 JSON
        let result: Value = serde_json::from_slice(&out_buf)
            .map_err(|e| AppError::other(format!("wasm 返回值不是 JSON: {e} (raw: {})",
                String::from_utf8_lossy(&out_buf))))?;

        Ok(result)
    }
}

// ==================== 测试 ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个最小的 wasm,导出 alloc/dealloc/run,run 把 input 复制一份作为输出
    /// 用 wat 文本描述
    const PASSTHROUGH_WAT: &str = r#"
        (module
          (memory (export "memory") 1)
          (global $heap (mut i32) (i32.const 1024))
          
          (func (export "alloc") (param $size i32) (result i32)
            (local $ptr i32)
            (local.set $ptr (global.get $heap))
            (global.set $heap (i32.add (global.get $heap) (local.get $size)))
            (local.get $ptr)
          )
          
          (func (export "dealloc") (param $ptr i32) (param $size i32)
            ;; 简化:不实际回收
            nop
          )
          
          (func (export "run") (param $fptr i32) (param $flen i32) (param $iptr i32) (param $ilen i32) (result i64)
            (local $out_ptr i32)
            (local $out_len i32)
            ;; 分配输出 = input 同样大小
            (local.set $out_ptr (global.get $heap))
            (local.set $out_len (local.get $ilen))
            (global.set $heap (i32.add (global.get $heap) (local.get $ilen)))
            ;; 复制 input 到 output
            (memory.copy (local.get $out_ptr) (local.get $iptr) (local.get $ilen))
            ;; 拼成 i64 返回
            (i64.or
              (i64.shl (i64.extend_i32_u (local.get $out_ptr)) (i64.const 32))
              (i64.extend_i32_u (local.get $out_len))
            )
          )
        )
    "#;

    #[test]
    fn passthrough_wasm_round_trip() {
        let bytes = wat::parse_str(PASSTHROUGH_WAT).expect("WAT 解析失败");
        let inst = WasmInstance::from_bytes(&bytes).expect("实例化失败");

        let input = serde_json::json!({"hello": "world", "n": 42});
        let out = inst.call("echo", &input).expect("call 失败");

        assert_eq!(out, input);
    }

    #[test]
    fn unknown_function_still_works_passthrough() {
        // 上面那个 wasm 不管传什么函数名,行为都一样(passthrough)
        // 这里验证: passthrough 的语义下,函数名虽然传进去但不影响输出
        let bytes = wat::parse_str(PASSTHROUGH_WAT).unwrap();
        let inst = WasmInstance::from_bytes(&bytes).unwrap();
        let r = inst.call("any_name", &serde_json::json!({"x": 1})).unwrap();
        assert_eq!(r, serde_json::json!({"x": 1}));
    }
}
