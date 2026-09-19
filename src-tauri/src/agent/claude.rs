//! claude CLI ランナー（担当: WS-E）。
//!
//! 実行: `claude -p <prompt> --output-format stream-json --verbose --permission-mode <mode> [--resume <id>]`
//!
//! 観測した stream-json（claude 2.1.x, fixture: `tests/fixtures/claude_stream.jsonl`）:
//! - `{"type":"system","subtype":"init","session_id":..,"model":..}` → SessionStarted
//! - `{"type":"system","subtype":"hook_*"|"thinking_tokens"}` / `rate_limit_event` → 無視
//! - `{"type":"assistant","message":{"content":[{type:text|thinking|tool_use}]}}` → AssistantText / Thinking / ToolUse
//! - `{"type":"user","message":{"content":[{type:"tool_result",tool_use_id,content,is_error}]}}` → ToolResult
//!   （content は文字列または `[{type:"text",text}]` 配列）
//! - `{"type":"result","subtype":"success"|"error_*","is_error","result","duration_ms","total_cost_usd","usage","session_id"}` → Result

use crate::models::{AgentKind, PermissionLevel};

use super::runner::{AgentRunner, CommandSpec, ParseState, RunSpec};
use super::types::AgentEvent;

pub struct ClaudeRunner;

impl AgentRunner for ClaudeRunner {
    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }

    fn build_command(&self, spec: &RunSpec) -> CommandSpec {
        let mode = match spec.permission {
            PermissionLevel::Safe => "acceptEdits",
            PermissionLevel::Full => "bypassPermissions",
        };
        let mut args: Vec<String> = vec![
            "-p".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--permission-mode".into(),
            mode.into(),
        ];
        if let Some(id) = &spec.resume_session_id {
            args.push("--resume".into());
            args.push(id.clone());
        }
        if let Some(m) = &spec.model {
            args.push("--model".into());
            args.push(m.clone());
        }
        // プロンプトは stdin で渡す（引数長制限・先頭 '-' 問題を回避）
        CommandSpec {
            program: "claude".into(),
            args,
            stdin: Some(spec.prompt.clone()),
        }
    }

    fn parse_line(&self, _line: &str, _state: &mut ParseState) -> Vec<AgentEvent> {
        // 仮実装: WS-E が fixture を使って実装する。
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn build_command_resume() {
        let spec = RunSpec {
            cwd: PathBuf::from("/tmp"),
            prompt: "hi".into(),
            resume_session_id: Some("abc".into()),
            permission: PermissionLevel::Safe,
            model: None,
        };
        let c = ClaudeRunner.build_command(&spec);
        assert_eq!(c.program, "claude");
        assert!(c.args.windows(2).any(|w| w == ["--resume", "abc"]));
        assert_eq!(c.stdin.as_deref(), Some("hi"));
    }
}
