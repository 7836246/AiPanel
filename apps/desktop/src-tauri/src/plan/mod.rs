//! Plan Engine.
//!
//! Turns a natural-language intent into a structured, reviewable [`Plan`]. The
//! agent's job (later) is to produce plans; AiPanel always converts them to this
//! shape and runs them through the Risk Reviewer before execution — the AI's
//! output is a plan, not a fact (docs/SECURITY_MODEL.zh-Hans.md).
//!
//! [`MockPlanEngine`] is a deterministic, offline stand-in: it routes on
//! keywords and only ever emits **read-only diagnostics**, so it can never
//! propose a destructive step (honoring intents like "不要删除任何文件").

use crate::core::error::AppResult;
use crate::core::types::{new_id, now, Plan, PlanStep};
use crate::risk::classify_command;

pub trait PlanEngine: Send + Sync {
    fn create_plan(&self, intent: &str, server_id: Option<&str>) -> AppResult<Plan>;
}

pub struct MockPlanEngine;

impl PlanEngine for MockPlanEngine {
    fn create_plan(&self, intent: &str, server_id: Option<&str>) -> AppResult<Plan> {
        let steps = pick_commands(intent)
            .into_iter()
            .map(|(summary, command)| {
                let risk = classify_command(command).level;
                PlanStep {
                    summary: summary.to_string(),
                    command: command.to_string(),
                    risk,
                    read_only: risk == crate::core::types::RiskLevel::Low,
                    tool: None,
                }
            })
            .collect();
        Ok(Plan {
            id: new_id(),
            server_id: server_id.map(|s| s.to_string()),
            goal: derive_goal(intent),
            steps,
            created_at: now(),
        })
    }
}

fn has_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// Route an intent to a read-only diagnostic command set.
fn pick_commands(intent: &str) -> Vec<(&'static str, &'static str)> {
    let i = intent.to_lowercase();

    if has_any(&i, &["网站", "打不开", "unreachable", "nginx", "502", "503", "网页", "站点"]) {
        return vec![
            ("检查 nginx 服务状态", "systemctl status nginx --no-pager"),
            ("检查监听端口", "ss -ltn"),
            ("查看 nginx 最近错误日志", "journalctl -u nginx -n 50 --no-pager"),
        ];
    }
    if has_any(&i, &["磁盘", "disk", "空间", "full", "存储"]) {
        return vec![
            ("查看磁盘使用", "df -h"),
            ("查看 inode 使用", "df -i"),
        ];
    }
    if has_any(&i, &["内存", "memory", "oom", "卡", "慢"]) {
        return vec![
            ("查看内存", "free -m"),
            ("查看负载", "cat /proc/loadavg"),
            ("查看占用最高的进程", "ps aux --sort=-%mem"),
        ];
    }
    if has_any(&i, &["docker", "容器", "container"]) {
        return vec![
            ("查看运行中的容器", "docker ps"),
            ("查看容器资源占用", "docker stats --no-stream"),
        ];
    }
    // Default: a general read-only health snapshot.
    vec![
        ("系统信息", "uname -rs"),
        ("运行时长", "uptime -p"),
        ("磁盘使用", "df -h"),
        ("内存", "free -m"),
        ("监听端口", "ss -ltn"),
    ]
}

fn derive_goal(intent: &str) -> String {
    let trimmed = intent.trim();
    let short: String = trimmed.chars().take(60).collect();
    if trimmed.chars().count() > 60 {
        format!("诊断：{short}…")
    } else {
        format!("诊断：{short}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::RiskLevel;

    #[test]
    fn mock_plans_are_read_only() {
        let e = MockPlanEngine;
        for intent in [
            "检查这个网站为什么打不开，不要删除任何文件",
            "磁盘快满了帮我看看",
            "服务器内存占用很高",
            "看看 docker 容器",
            "随便看看",
        ] {
            let plan = e.create_plan(intent, Some("srv")).unwrap();
            assert!(!plan.steps.is_empty());
            assert!(
                plan.steps.iter().all(|s| s.read_only && s.risk == RiskLevel::Low),
                "intent {intent:?} produced a non-read-only step"
            );
        }
    }

    #[test]
    fn routes_web_intent_to_nginx_checks() {
        let plan = MockPlanEngine.create_plan("网站打不开了", Some("s")).unwrap();
        assert!(plan.steps.iter().any(|s| s.command.contains("nginx")));
    }

    #[test]
    fn goal_restates_intent() {
        let plan = MockPlanEngine.create_plan("检查磁盘", None).unwrap();
        assert!(plan.goal.starts_with("诊断："));
        assert_eq!(plan.server_id, None);
    }
}
