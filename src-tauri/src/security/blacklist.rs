//! 危险命令黑名单
//!
//! Phase 1 实现：
//! - 内置正则黑名单
//! - 数据库自定义规则（Phase 1 后期）
//! - 危险命令确认（前端层）

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// 内置危险模式（使用 owned String 便于序列化）
pub fn default_blacklist() -> Vec<BlacklistRule> {
    vec![
        BlacklistRule {
            pattern: r"(^|\s)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r|-rf|-fr)\s+/\s*$"
                .to_string(),
            description: "rm -rf / 根目录删除".to_string(),
        },
        BlacklistRule {
            pattern: r"(^|\s)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r|-rf|-fr)\s+~"
                .to_string(),
            description: "rm -rf ~ 家目录删除".to_string(),
        },
        BlacklistRule {
            pattern: r"(^|\s)(format|diskpart)\s+[a-zA-Z]:".to_string(),
            description: "format 格式化磁盘".to_string(),
        },
        BlacklistRule {
            pattern: r"(^|\s)del\s+/[fFsS]\s+/[qQ]\s+[a-zA-Z]:\\".to_string(),
            description: "del /f /s /q 强制删除".to_string(),
        },
        BlacklistRule {
            pattern: r"dd\s+if=.*\s+of=/dev/(sd|hd|nvme|vd)".to_string(),
            description: "dd 写磁盘设备".to_string(),
        },
        BlacklistRule {
            pattern: r":\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:".to_string(),
            description: "Fork bomb".to_string(),
        },
        BlacklistRule {
            pattern: r"mkfs(\.[a-z0-9]+)?\s+/dev/".to_string(),
            description: "mkfs 格式化".to_string(),
        },
        BlacklistRule {
            pattern: r">\s*/dev/sd[a-z]".to_string(),
            description: "重定向到块设备".to_string(),
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistRule {
    pub pattern: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistHit {
    pub rule: BlacklistRule,
    pub matched_text: String,
}

/// 编译后的缓存
struct Compiled {
    regexes: Vec<(Regex, BlacklistRule)>,
}

static COMPILED: OnceLock<Compiled> = OnceLock::new();

fn compiled() -> &'static Compiled {
    COMPILED.get_or_init(|| {
        let mut regexes = Vec::new();
        for rule in default_blacklist() {
            // 用宽松模式，允许单行匹配
            match Regex::new(&format!("(?i){}", rule.pattern)) {
                Ok(re) => regexes.push((re, rule)),
                Err(e) => {
                    tracing::error!("编译黑名单规则失败: {}: {}", rule.pattern, e);
                }
            }
        }
        Compiled { regexes }
    })
}

/// 检查命令是否命中黑名单
pub fn check(command: &str) -> Option<BlacklistHit> {
    for (re, rule) in &compiled().regexes {
        if let Some(m) = re.find(command) {
            return Some(BlacklistHit {
                rule: rule.clone(),
                matched_text: m.as_str().to_string(),
            });
        }
    }
    None
}

/// 启发式：检查命令是否包含「危险」关键字，需要用户二次确认
pub fn is_dangerous_heuristic(command: &str) -> bool {
    let keywords = [
        r"\brm\b",
        r"\bdel\b",
        r"\bdrop\b",
        r"\btruncate\b",
        r"\bformat\b",
        r"\bdiskpart\b",
        r"\bshutdown\b",
        r"\breboot\b",
        r"\bkill\s+-9\b",
        r"\bgit\s+push\s+(-f|--force)\b",
        r"\bgit\s+reset\s+--hard\b",
    ];
    let re = Regex::new(&format!("(?i)({})", keywords.join("|"))).unwrap();
    re.is_match(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blacklist_blocks_rm_rf_root() {
        let hit = check("rm -rf /");
        assert!(hit.is_some());
    }

    #[test]
    fn test_blacklist_blocks_format() {
        let hit = check("format c:");
        assert!(hit.is_some());
    }

    #[test]
    fn test_blacklist_passes_safe() {
        let hit = check("ls -la /tmp");
        assert!(hit.is_none());
    }

    #[test]
    fn test_heuristic_rm() {
        assert!(is_dangerous_heuristic("rm -rf /tmp/cache"));
    }

    #[test]
    fn test_heuristic_safe() {
        assert!(!is_dangerous_heuristic("echo hello"));
    }
}
