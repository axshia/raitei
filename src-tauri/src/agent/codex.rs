//! codex CLI ランナー（担当: WS-E）。
//!
//! 実行:
//! - 新規: `codex exec --json --skip-git-repo-check <sandbox opts> -`（プロンプトは stdin）
//! - 継続: `codex exec resume --json --skip-git-repo-check <sandbox opts> <thread_id> -`
//!   （codex-cli 0.155.1 で `exec resume` が `--json` / `-c sandbox_mode=..` を受け付け、
//!   同じ thread_id で会話が継続することを実機確認済み）
//!
//! 観測した JSONL（codex-cli 0.155, fixture: `tests/fixtures/codex_*.jsonl`）:
//! - `{"type":"thread.started","thread_id":..}` → SessionStarted（resume でも同じ thread_id が出る）
//! - `{"type":"turn.started"}` / `item.updated` → 無視
//! - `{"type":"item.started","item":{"type":"command_execution"|"file_change"|..}}` → ToolUse
//! - `{"type":"item.completed","item":{"type":"command_execution","aggregated_output","exit_code"}}` → ToolResult
//! - `{"type":"item.completed","item":{"type":"file_change","changes":[{path,kind}],"status"}}` → ToolResult
//! - `{"type":"item.completed","item":{"type":"agent_message","text"}}` → AssistantText
//! - `{"type":"item.completed","item":{"type":"reasoning","text"}}` → Thinking
//! - `{"type":"item.completed","item":{"type":"error","message"}}` → Error（警告。継続する）
//! - `{"type":"turn.completed","usage":{..}}` → Result(is_error=false, text=last_text)
//! - `{"type":"error","message"}` → Error / `{"type":"turn.failed","error":{"message"}}` → Result(is_error=true)
//!   （message は API のエラー JSON を文字列化したものなので、内側の `error.message` を取り出す）
//! - 存在しない thread_id を resume した場合は JSONL を 1 行も出さず、stderr に
//!   `Error: thread/resume: ... no rollout found ...` を出して終了コード 1 で終わる（manager 側で扱う）
//!
//! ツール系 item（command_execution / file_change / mcp_tool_call / web_search / todo_list）は
//! `item.started` で ToolUse、`item.completed` で ToolResult に対応させる。`item.started` を伴わない
//! 完了だけの item は ToolResult だけになる（フロントは対応する ToolUse の無い結果も表示する）。

use serde_json::{json, Value};

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

    fn parse_line(&self, line: &str, state: &mut ParseState) -> Vec<AgentEvent> {
        let Some(v) = serde_json::from_str::<Value>(line.trim()).ok().filter(Value::is_object) else {
            return Vec::new();
        };
        let item = v.get("item").unwrap_or(&Value::Null);
        match str_field(&v, "type") {
            Some("thread.started") => match str_field(&v, "thread_id").filter(|s| !s.is_empty()) {
                Some(id) => {
                    state.session_id = Some(id.to_string());
                    vec![AgentEvent::SessionStarted {
                        session_id: id.to_string(),
                        model: None,
                    }]
                }
                None => Vec::new(),
            },
            Some("item.started") => tool_use(item).into_iter().collect(),
            Some("item.completed") => item_completed(item, state),
            Some("turn.completed") => vec![AgentEvent::Result {
                is_error: false,
                text: state.last_text.clone(),
                duration_ms: None,
                cost_usd: None,
                usage: v.get("usage").filter(|u| !u.is_null()).cloned(),
            }],
            Some("turn.failed") => vec![AgentEvent::Result {
                is_error: true,
                text: Some(error_message(v.get("error").and_then(|e| e.get("message")))),
                duration_ms: None,
                cost_usd: None,
                usage: None,
            }],
            Some("error") => vec![AgentEvent::Error {
                message: error_message(v.get("message")),
            }],
            _ => Vec::new(),
        }
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn item_completed(item: &Value, state: &mut ParseState) -> Vec<AgentEvent> {
    match str_field(item, "type") {
        Some("agent_message") => match str_field(item, "text").filter(|t| !t.trim().is_empty()) {
            Some(text) => {
                state.last_text = Some(text.to_string());
                vec![AgentEvent::AssistantText { text: text.to_string() }]
            }
            None => Vec::new(),
        },
        Some("reasoning") => match str_field(item, "text").filter(|t| !t.trim().is_empty()) {
            Some(text) => vec![AgentEvent::Thinking { text: text.to_string() }],
            None => Vec::new(),
        },
        Some("error") => vec![AgentEvent::Error {
            message: error_message(item.get("message")),
        }],
        _ => tool_result(item).into_iter().collect(),
    }
}

fn item_id(item: &Value) -> String {
    str_field(item, "id").unwrap_or_default().to_string()
}

/// ツール系 item の開始を ToolUse にする。ツール以外（agent_message 等）・未知の種別は None。
fn tool_use(item: &Value) -> Option<AgentEvent> {
    let (name, input) = match str_field(item, "type")? {
        "command_execution" => ("shell".to_string(), json!({ "command": str_field(item, "command").unwrap_or_default() })),
        "file_change" => ("file_change".to_string(), json!({ "changes": item.get("changes").cloned().unwrap_or(json!([])) })),
        "mcp_tool_call" => (
            format!(
                "mcp__{}__{}",
                str_field(item, "server").unwrap_or("unknown"),
                str_field(item, "tool").unwrap_or("unknown")
            ),
            item.get("arguments").filter(|a| !a.is_null()).cloned().unwrap_or(json!({})),
        ),
        "web_search" => ("web_search".to_string(), json!({ "query": str_field(item, "query").unwrap_or_default() })),
        "todo_list" => ("todo_list".to_string(), json!({ "items": item.get("items").cloned().unwrap_or(json!([])) })),
        _ => return None,
    };
    Some(AgentEvent::ToolUse {
        id: item_id(item),
        name,
        input,
    })
}

/// ツール系 item の完了を ToolResult にする。ツール以外・未知の種別は None。
fn tool_result(item: &Value) -> Option<AgentEvent> {
    let status = str_field(item, "status").unwrap_or_default();
    let failed = matches!(status, "failed" | "declined");
    let (output, is_error) = match str_field(item, "type")? {
        "command_execution" => {
            let exit_code = item.get("exit_code").and_then(Value::as_i64);
            (
                str_field(item, "aggregated_output").unwrap_or_default().to_string(),
                failed || exit_code.is_some_and(|c| c != 0),
            )
        }
        "file_change" => {
            let lines: Vec<String> = item
                .get("changes")
                .and_then(Value::as_array)
                .map(|changes| {
                    changes
                        .iter()
                        .map(|c| {
                            format!(
                                "{} {}",
                                str_field(c, "kind").unwrap_or("update"),
                                str_field(c, "path").unwrap_or_default()
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            (lines.join("\n"), failed)
        }
        "mcp_tool_call" => {
            let error = item
                .get("error")
                .filter(|e| !e.is_null())
                .map(|e| str_field(e, "message").map(String::from).unwrap_or_else(|| e.to_string()));
            match error {
                Some(message) => (message, true),
                None => (mcp_result_text(item.get("result")), failed),
            }
        }
        "web_search" => (String::new(), failed),
        "todo_list" => {
            let lines: Vec<String> = item
                .get("items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|t| {
                            let done = t.get("completed").and_then(Value::as_bool).unwrap_or(false);
                            format!("[{}] {}", if done { "x" } else { " " }, str_field(t, "text").unwrap_or_default())
                        })
                        .collect()
                })
                .unwrap_or_default();
            (lines.join("\n"), failed)
        }
        _ => return None,
    };
    Some(AgentEvent::ToolResult {
        tool_use_id: item_id(item),
        output,
        is_error,
    })
}

/// mcp_tool_call の result（`{content:[{type:"text",text}]}` など）を文字列にする。
fn mcp_result_text(result: Option<&Value>) -> String {
    let Some(result) = result.filter(|r| !r.is_null()) else {
        return String::new();
    };
    let texts: Vec<&str> = result
        .get("content")
        .and_then(Value::as_array)
        .map(|blocks| blocks.iter().filter_map(|b| str_field(b, "text")).collect())
        .unwrap_or_default();
    if texts.is_empty() {
        result.to_string()
    } else {
        texts.join("\n")
    }
}

/// codex のエラーメッセージ。API エラーの JSON が文字列で入っている場合は内側の message を取り出す。
fn error_message(message: Option<&Value>) -> String {
    let raw = match message {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => return "不明なエラー".to_string(),
        Some(other) => other.to_string(),
    };
    serde_json::from_str::<Value>(&raw)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .or_else(|| v.get("message"))
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn parse_all(text: &str) -> (Vec<AgentEvent>, ParseState) {
        let mut state = ParseState::default();
        let events = text.lines().flat_map(|l| CodexRunner.parse_line(l, &mut state)).collect();
        (events, state)
    }

    /// イベントの `type` タグ（snake_case）。
    fn kind(e: &AgentEvent) -> String {
        serde_json::to_value(e).unwrap()["type"].as_str().unwrap().to_string()
    }

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
        assert!(c.args.windows(2).any(|w| w == ["-c", "sandbox_mode=\"workspace-write\""]));
        assert_eq!(c.args.last().unwrap(), "-");
        assert_eq!(c.stdin.as_deref(), Some("hi"));

        spec.resume_session_id = Some("t1".into());
        spec.permission = PermissionLevel::Full;
        let c = CodexRunner.build_command(&spec);
        assert_eq!(&c.args[..3], &["exec", "resume", "--json"]);
        assert!(c.args.iter().any(|a| a == "--dangerously-bypass-approvals-and-sandbox"));
        assert_eq!(c.args[c.args.len() - 2], "t1");
    }

    #[test]
    fn parse_exec_fixture() {
        let (events, state) = parse_all(&fixture("codex_exec.jsonl"));
        let tid = "01a0b71b-1e7c-71a3-8b0d-a30edaea5764";
        assert_eq!(state.session_id.as_deref(), Some(tid));
        let kinds: Vec<_> = events.iter().map(kind).collect();
        assert_eq!(
            kinds,
            ["session_started", "error", "assistant_text", "tool_use", "tool_result", "assistant_text", "result"]
        );
        assert_eq!(
            events[0],
            AgentEvent::SessionStarted {
                session_id: tid.into(),
                model: None,
            }
        );
        assert!(matches!(&events[1], AgentEvent::Error { message } if message.starts_with("Under-development features")));
        assert_eq!(
            events[3],
            AgentEvent::ToolUse {
                id: "item_3".into(),
                name: "shell".into(),
                input: json!({ "command": "/bin/zsh -lc 'echo hi'" }),
            }
        );
        assert_eq!(
            events[4],
            AgentEvent::ToolResult {
                tool_use_id: "item_3".into(),
                output: "hi\n".into(),
                is_error: false,
            }
        );
        match &events[6] {
            AgentEvent::Result {
                is_error, text, usage, ..
            } => {
                assert!(!is_error);
                // turn.completed には本文が無いので最後の agent_message を使う
                assert_eq!(text.as_deref(), Some("done"));
                assert_eq!(usage.as_ref().unwrap()["output_tokens"], 293);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parse_resume_fixture_keeps_same_thread() {
        let (events, state) = parse_all(&fixture("codex_resume.jsonl"));
        assert_eq!(state.session_id.as_deref(), Some("01a0b72c-abf8-7560-9385-7cd16d98cce5"));
        let kinds: Vec<_> = events.iter().map(kind).collect();
        assert_eq!(kinds, ["session_started", "error", "assistant_text", "result"]);
        assert!(matches!(&events[3], AgentEvent::Result { text: Some(t), .. } if t == "bee"));
    }

    #[test]
    fn parse_file_change_fixture() {
        let (events, _) = parse_all(&fixture("codex_file_change.jsonl"));
        let kinds: Vec<_> = events.iter().map(kind).collect();
        assert_eq!(
            kinds,
            [
                "session_started",
                "error",
                "assistant_text",
                "thinking",
                "tool_use",
                "tool_result",
                "assistant_text",
                "result"
            ]
        );
        assert_eq!(events[3], AgentEvent::Thinking { text: "**Preparing file update**".into() });
        assert_eq!(
            events[4],
            AgentEvent::ToolUse {
                id: "item_3".into(),
                name: "file_change".into(),
                input: json!({ "changes": [{ "path": "/private/tmp/raitei-wse-probe/a.txt", "kind": "add" }] }),
            }
        );
        assert_eq!(
            events[5],
            AgentEvent::ToolResult {
                tool_use_id: "item_3".into(),
                output: "add /private/tmp/raitei-wse-probe/a.txt".into(),
                is_error: false,
            }
        );
    }

    #[test]
    fn parse_turn_failed_fixture() {
        let (events, _) = parse_all(&fixture("codex_turn_failed.jsonl"));
        let kinds: Vec<_> = events.iter().map(kind).collect();
        assert_eq!(kinds, ["session_started", "error", "error", "error", "result"]);
        let expected = "The 'no-such-model-xyz' model is not supported when using Codex with a ChatGPT account.";
        assert_eq!(events[3], AgentEvent::Error { message: expected.into() });
        match &events[4] {
            AgentEvent::Result { is_error, text, .. } => {
                assert!(is_error);
                assert_eq!(text.as_deref(), Some(expected));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn failed_command_is_error() {
        let mut st = ParseState::default();
        let ev = CodexRunner.parse_line(
            r#"{"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"false","aggregated_output":"","exit_code":1,"status":"failed"}}"#,
            &mut st,
        );
        assert_eq!(
            ev,
            vec![AgentEvent::ToolResult {
                tool_use_id: "i1".into(),
                output: String::new(),
                is_error: true,
            }]
        );
    }

    #[test]
    fn mcp_and_todo_items() {
        let mut st = ParseState::default();
        let ev = CodexRunner.parse_line(
            r#"{"type":"item.started","item":{"id":"m1","type":"mcp_tool_call","server":"docs","tool":"search","arguments":{"q":"x"},"result":null,"error":null,"status":"in_progress"}}"#,
            &mut st,
        );
        assert_eq!(
            ev,
            vec![AgentEvent::ToolUse {
                id: "m1".into(),
                name: "mcp__docs__search".into(),
                input: json!({ "q": "x" }),
            }]
        );
        let ev = CodexRunner.parse_line(
            r#"{"type":"item.completed","item":{"id":"m1","type":"mcp_tool_call","server":"docs","tool":"search","arguments":{"q":"x"},"result":{"content":[{"type":"text","text":"found"}]},"error":null,"status":"completed"}}"#,
            &mut st,
        );
        assert_eq!(
            ev,
            vec![AgentEvent::ToolResult {
                tool_use_id: "m1".into(),
                output: "found".into(),
                is_error: false,
            }]
        );
        let ev = CodexRunner.parse_line(
            r#"{"type":"item.completed","item":{"id":"t1","type":"todo_list","items":[{"text":"a","completed":true},{"text":"b","completed":false}]}}"#,
            &mut st,
        );
        assert_eq!(
            ev,
            vec![AgentEvent::ToolResult {
                tool_use_id: "t1".into(),
                output: "[x] a\n[ ] b".into(),
                is_error: false,
            }]
        );
    }

    #[test]
    fn ignores_noise_and_unknown() {
        let mut st = ParseState::default();
        for line in [
            "",
            "Reading prompt from stdin...",
            r#"{"type":"turn.started"}"#,
            r#"{"type":"item.updated","item":{"id":"t1","type":"todo_list","items":[]}}"#,
            r#"{"type":"item.started","item":{"id":"x","type":"agent_message","text":""}}"#,
            r#"{"type":"item.completed","item":{"id":"x","type":"future_item"}}"#,
            r#"{"type":"item.completed","item":{"id":"x","type":"agent_message","text":" "}}"#,
        ] {
            assert!(CodexRunner.parse_line(line, &mut st).is_empty(), "{line}");
        }
    }
}
