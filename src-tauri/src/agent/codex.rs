//! codex CLI ランナー（担当: WS-E）。
//!
//! 実行:
//! - 新規: `codex exec --json --skip-git-repo-check <sandbox opts> -`（プロンプトは stdin）
//! - 継続: `codex exec resume --json --skip-git-repo-check <sandbox opts> <thread_id> -`
//!
//! 観測した JSONL（codex-cli 0.155, fixture: `tests/fixtures/codex_exec.jsonl`）:
//! - `{"type":"thread.started","thread_id":..}` → SessionStarted
//! - `{"type":"turn.started"}` → 無視
//! - `{"type":"item.started","item":{"type":"command_execution","id","command",..}}` → ToolUse(name="shell")
//! - `{"type":"item.completed","item":{"type":"command_execution","aggregated_output","exit_code"}}` → ToolResult
//! - `{"type":"item.completed","item":{"type":"agent_message","text"}}` → AssistantText
//! - `{"type":"item.completed","item":{"type":"reasoning","text"}}` → Thinking
//! - `{"type":"item.completed","item":{"type":"file_change"|"mcp_tool_call"|"web_search"|"todo_list",..}}` → ToolUse+ToolResult
//! - `{"type":"item.completed","item":{"type":"error","message"}}` → Error（警告。継続する）
//! - `{"type":"turn.completed","usage":{..}}` → Result(is_error=false, text=last_text)
//! - `{"type":"turn.failed","error":{"message"}}` / `{"type":"error","message"}` → Result(is_error=true) / Error
//!
//! 注意: `exec resume` が `--json` / `-s` を受け付けるかはバージョン依存。WS-E が実機で確認すること。

use crate::models::{AgentKind, PermissionLevel};

use super::runner::{AgentRunner, CommandSpec, ParseState, RunSpec};
use super::types::AgentEvent;

pub struct CodexRunner;

impl AgentRunner for CodexRunner {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }

    fn build_command(&self, spec: &RunSpec) -> CommandSpec {
        let mut args: Vec<String> = vec!["exec".into()];
        if spec.resume_session_id.is_some() {
            args.push("resume".into());
        }
        args.push("--json".into());
        args.push("--skip-git-repo-check".into());
        match spec.permission {
            PermissionLevel::Safe => {
                args.push("-c".into());
                args.push("sandbox_mode=\"workspace-write\"".into());
            }
            PermissionLevel::Full => args.push("--dangerously-bypass-approvals-and-sandbox".into()),
        }
        if let Some(m) = &spec.model {
            args.push("-m".into());
            args.push(m.clone());
        }
        if let Some(id) = &spec.resume_session_id {
            args.push(id.clone());
        }
        args.push("-".into());
        CommandSpec {
            program: "codex".into(),
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
    fn build_command_new_and_resume() {
        let mut spec = RunSpec {
            cwd: PathBuf::from("/tmp"),
            prompt: "hi".into(),
            resume_session_id: None,
            permission: PermissionLevel::Safe,
            model: None,
        };
        let c = CodexRunner.build_command(&spec);
        assert_eq!(&c.args[..2], &["exec", "--json"]);
        assert_eq!(c.args.last().unwrap(), "-");

        spec.resume_session_id = Some("t1".into());
        let c = CodexRunner.build_command(&spec);
        assert_eq!(&c.args[..3], &["exec", "resume", "--json"]);
        assert_eq!(c.args[c.args.len() - 2], "t1");
    }
}
