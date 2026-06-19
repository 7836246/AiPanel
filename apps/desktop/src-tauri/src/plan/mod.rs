//! 计划引擎（Plan Engine）。
//!
//! 将自然语言意图转换成结构化、可审查的 [`Plan`]。Agent（后续）的职责是产出
//! 计划；AiPanel 始终把它们转换成这种结构，并在执行前先过 Risk Reviewer——
//! AI 的输出是计划，不是事实（docs/SECURITY_MODEL.zh-Hans.md）。
//!
//! [`MockPlanEngine`] 是一个确定性的离线替身：它基于关键词路由，且只产出
//! **只读诊断命令**，因此永远不会提出破坏性步骤（尊重诸如「不要删除任何文件」
//! 这类意图）。

use crate::core::error::AppResult;
use crate::core::types::{new_id, now, Plan, PlanStep};
use crate::risk::classify_command;

/// 计划引擎抽象：把意图转换为结构化计划。可由不同实现（Mock / 真实 Agent）提供。
pub trait PlanEngine: Send + Sync {
    fn create_plan(&self, intent: &str, server_id: Option<&str>) -> AppResult<Plan>;
}

/// 离线 Mock 计划引擎：只产出只读诊断步骤。
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

/// 判断 haystack 是否包含 needles 中的任意一个子串。
fn has_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// 将意图路由到一组只读诊断命令。
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
    // 默认：一份通用的只读健康快照。
    vec![
        ("系统信息", "uname -rs"),
        ("运行时长", "uptime -p"),
        ("磁盘使用", "df -h"),
        ("内存", "free -m"),
        ("监听端口", "ss -ltn"),
    ]
}

/// 由意图推导出计划目标文案（过长时截断到 60 个字符并加省略号）。
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

    #[test]
    fn routes_disk_memory_docker_intents() {
        // 关键词分别路由到对应的只读探测命令。
        let disk = MockPlanEngine.create_plan("磁盘空间不够了", Some("s")).unwrap();
        assert!(disk.steps.iter().any(|s| s.command.starts_with("df")));

        let mem = MockPlanEngine.create_plan("内存 oom 了", Some("s")).unwrap();
        assert!(mem.steps.iter().any(|s| s.command.contains("free -m")));

        let docker = MockPlanEngine.create_plan("看下 container 状态", Some("s")).unwrap();
        assert!(docker.steps.iter().any(|s| s.command.contains("docker ps")));
    }

    #[test]
    fn unmatched_intent_falls_back_to_generic_snapshot() {
        // 无关键词命中 → 通用只读快照(含 uname / ss 等)。
        let plan = MockPlanEngine.create_plan("帮我大致瞧一眼这台机器", Some("s")).unwrap();
        let cmds: Vec<&str> = plan.steps.iter().map(|s| s.command.as_str()).collect();
        assert!(cmds.iter().any(|c| c.contains("uname")));
        assert!(cmds.iter().any(|c| c.contains("ss -ltn")));
    }

    #[test]
    fn goal_truncates_long_intent_with_ellipsis() {
        let long = "检查".repeat(50); // 100 个字符,超过 60 上限
        let plan = MockPlanEngine.create_plan(&long, None).unwrap();
        assert!(plan.goal.ends_with('…'));
        // 目标 = "诊断："(3) + 截断的 60 字符 + "…"(1)
        assert_eq!(plan.goal.chars().count(), 3 + 60 + 1);
    }
}
