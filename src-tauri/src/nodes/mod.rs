//! 节点类型注册表
//!
//! 包含 9 种内置节点：cmd / script / http / ai / file / delay / condition / loop / subworkflow

pub mod ai;
pub mod cmd;
pub mod condition;
pub mod delay;
pub mod file;
pub mod http;
pub mod loop_node;
pub mod node;
pub mod registry;
pub mod script;
pub mod subworkflow;
