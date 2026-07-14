//! 统一错误类型
//!
//! 前后端共享一个错误模型，便于前端做提示。

use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("连接池错误: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("未找到: {0}")]
    NotFound(String),

    #[error("参数无效: {0}")]
    InvalidInput(String),

    #[error("DAG 错误: {0}")]
    Dag(String),

    #[error("执行错误: {0}")]
    Execution(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("其他: {0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serde_json::Map::new();
        s.insert("kind".into(), serde_json::Value::String(self.kind().into()));
        s.insert("message".into(), serde_json::Value::String(self.to_string()));
        serde_json::Value::Object(s).serialize(serializer)
    }
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Database(_) => "database",
            AppError::Pool(_) => "pool",
            AppError::NotFound(_) => "not_found",
            AppError::InvalidInput(_) => "invalid_input",
            AppError::Dag(_) => "dag",
            AppError::Execution(_) => "execution",
            AppError::Io(_) => "io",
            AppError::Serde(_) => "serde",
            AppError::Other(_) => "other",
        }
    }

    pub fn not_found<T: Into<String>>(s: T) -> Self {
        AppError::NotFound(s.into())
    }

    pub fn invalid<T: Into<String>>(s: T) -> Self {
        AppError::InvalidInput(s.into())
    }

    pub fn dag<T: Into<String>>(s: T) -> Self {
        AppError::Dag(s.into())
    }

    pub fn other<T: Into<String>>(s: T) -> Self {
        AppError::Other(s.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;
