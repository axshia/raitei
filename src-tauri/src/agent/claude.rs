//! claude CLI ランナー（担当: WS-E）。
//!
//! 実行: `claude -p --output-format stream-json --verbose --permission-mode <mode> [--resume <id>]`
//! （プロンプトは stdin）
//!
//! 観測した stream-json（claude 2.1.277, fixture: `tests/fixtures/claude_*.jsonl`）:
//! - `{"type":"system","subtype":"init","session_id":..,"model":..}` → SessionStarted
//! - `{"type":"system","subtype":"hook_*"|"thinking_tokens"}` / `rate_limit_event` → 無視
//! - `{"type":"assistant","message":{"content":[{type:text|thinking|tool_use}]}}` → AssistantText / Thinking / ToolUse
//!   （content ブロックは 1 行に 1 つずつ届く。thinking は本文が空で署名だけのことが多いので空なら送らない）
//! - `{"type":"user","message":{"content":[{type:"tool_result",tool_use_id,content,is_error}]}}` → ToolResult
//!   （content は文字列または `[{type:"text",text}]` 配列）
//! - `{"type":"result","subtype":"success"|"error_*","is_error","result","duration_ms","total_cost_usd","usage"}` → Result
//!   - `--resume` の ID が存在しない場合は init を出さずに `subtype:"error_during_execution"`, `errors:[..]`
//!     の result 行だけを出して終了コード 1 で終わる（`result` フィールドは無い）
//!   - `permission_denials` が空でなければ（safe モードで Bash が拒否された等）、Result の前に Error を出す。
//!     拒否されたツール呼び出し自体は `tool_result`（`is_error:true`, "This command requires approval"）として届く
//! - `parent_tool_use_id` が付いた assistant / user 行はサブエージェント内部のやり取りなので無視する
//!   （親の Task ツールの ToolUse / ToolResult だけを表示する）

use serde_json::Value;

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

    fn parse_line(&self, line: &str, state: &mut ParseState) -> Vec<AgentEvent> {
        let Some(v) = parse_json_object(line) else {
            return Vec::new();
        };
        match v.get("type").and_then(Value::as_str) {
            Some("system") => parse_system(&v, state),
            Some("assistant") => parse_assistant(&v, state),
            Some("user") => parse_user(&v),
            Some("result") => parse_result(&v),
            _ => Vec::new(),
        }
    }
}

fn parse_json_object(line: &str) -> Option<Value> {
    serde_json::from_str::<Value>(line.trim()).ok().filter(Value::is_object)
}

fn str_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// サブエージェント（Task ツール）内部の行か。
fn is_sidechain(v: &Value) -> bool {
    v.get("parent_tool_use_id").is_some_and(|p| !p.is_null())
}

fn content_blocks(v: &Value) -> &[Value] {
    v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn parse_system(v: &Value, state: &mut ParseState) -> Vec<AgentEvent> {
    if str_field(v, "subtype") != Some("init") {
        return Vec::new();
    }
    let Some(session_id) = str_field(v, "session_id").filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    state.session_id = Some(session_id.to_string());
    vec![AgentEvent::SessionStarted {
        session_id: session_id.to_string(),
        model: str_field(v, "model").map(String::from),
    }]
}

fn parse_assistant(v: &Value, state: &mut ParseState) -> Vec<AgentEvent> {
    if is_sidechain(v) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for block in content_blocks(v) {
        match str_field(block, "type") {
            Some("text") => {
                if let Some(text) = str_field(block, "text").filter(|t| !t.trim().is_empty()) {
                    state.last_text = Some(text.to_string());
                    out.push(AgentEvent::AssistantText { text: text.to_string() });
                }
            }
            Some("thinking") => {
                if let Some(text) = str_field(block, "thinking").filter(|t| !t.trim().is_empty()) {
                    out.push(AgentEvent::Thinking { text: text.to_string() });
                }
            }
            Some("tool_use") | Some("server_tool_use") | Some("mcp_tool_use") => {
                out.push(AgentEvent::ToolUse {
                    id: str_field(block, "id").unwrap_or_default().to_string(),
                    name: str_field(block, "name").unwrap_or("unknown").to_string(),
                    input: block.get("input").cloned().unwrap_or_else(|| Value::Object(Default::default())),
                });
            }
            _ => {}
        }
    }
    out
}

fn parse_user(v: &Value) -> Vec<AgentEvent> {
    if is_sidechain(v) {
        return Vec::new();
    }
    content_blocks(v)
        .iter()
        .filter(|b| str_field(b, "type") == Some("tool_result"))
        .map(|b| AgentEvent::ToolResult {
            tool_use_id: str_field(b, "tool_use_id").unwrap_or_default().to_string(),
            output: tool_result_text(b.get("content")),
            is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect()
}

/// tool_result の content（文字列 / ブロック配列）を表示用の文字列にする。
fn tool_result_text(content: Option<&Value>) -> String {
    match content {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| match str_field(b, "type") {
                Some("text") => str_field(b, "text").map(String::from),
                Some("image") => Some("[image]".to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => other.to_string(),
    }
}

fn parse_result(v: &Value) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    if let Some(message) = permission_denials_message(v.get("permission_denials")) {
        out.push(AgentEvent::Error { message });
    }
    let is_error = v
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| str_field(v, "subtype") != Some("success"));
    let text = str_field(v, "result").map(String::from).or_else(|| {
        let errors: Vec<&str> = v
            .get("errors")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        (!errors.is_empty()).then(|| errors.join("\n"))
    });
    out.push(AgentEvent::Result {
        is_error,
        text,
        duration_ms: v.get("duration_ms").and_then(Value::as_u64),
        cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
        usage: v.get("usage").filter(|u| !u.is_null()).cloned(),
    });
    out
}

/// `permission_denials: [{tool_name, tool_use_id, tool_input}]` を 1 つのメッセージにまとめる。
fn permission_denials_message(denials: Option<&Value>) -> Option<String> {
    let denials = denials?.as_array().filter(|a| !a.is_empty())?;
    let items: Vec<String> = denials
        .iter()
        .map(|d| {
            let name = str_field(d, "tool_name").unwrap_or("unknown");
            let input = d.get("tool_input");
            let detail = ["command", "file_path", "path", "url", "pattern"]
                .iter()
                .find_map(|k| input.and_then(|i| str_field(i, k)));
            match detail {
                Some(detail) => format!("{name}: {detail}"),
                None => name.to_string(),
            }
        })
        .collect();
    Some(format!(
        "権限の制限により実行されなかった操作があります（{} 件）。必要ならタスクの権限レベルを変更して送り直してください。\n{}",
        items.len(),
        items.iter().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn parse_all(text: &str) -> (Vec<AgentEvent>, ParseState) {
        let mut state = ParseState::default();
        let events = text.lines().flat_map(|l| ClaudeRunner.parse_line(l, &mut state)).collect();
        (events, state)
    }

    fn spec(resume: Option<&str>, permission: PermissionLevel) -> RunSpec {
        RunSpec {
            cwd: PathBuf::from("/tmp"),
            prompt: "hi".into(),
            resume_session_id: resume.map(String::from),
            permission,
            model: None,
        }
    }

    #[test]
    fn build_command_resume() {
        let c = ClaudeRunner.build_command(&spec(Some("abc"), PermissionLevel::Safe));
        assert_eq!(c.program, "claude");
        assert!(c.args.windows(2).any(|w| w == ["--resume", "abc"]));
        assert!(c.args.windows(2).any(|w| w == ["--permission-mode", "acceptEdits"]));
        assert_eq!(c.stdin.as_deref(), Some("hi"));
    }

    #[test]
    fn build_command_new_full() {
        let c = ClaudeRunner.build_command(&spec(None, PermissionLevel::Full));
        assert_eq!(&c.args[..4], &["-p", "--output-format", "stream-json", "--verbose"]);
        assert!(c.args.windows(2).any(|w| w == ["--permission-mode", "bypassPermissions"]));
        assert!(!c.args.iter().any(|a| a == "--resume"));
    }

    #[test]
    fn parse_stream_fixture() {
        let (events, state) = parse_all(&fixture("claude_stream.jsonl"));
        let sid = "67c70e97-50d2-481f-a02f-9b7e03fc15ac";
        assert_eq!(state.session_id.as_deref(), Some(sid));
        assert_eq!(state.last_text.as_deref(), Some("done"));
        // rate_limit_event / thinking_tokens / 空の thinking は出さない
        assert_eq!(events.len(), 5, "{events:#?}");
        assert_eq!(
            events[0],
            AgentEvent::SessionStarted {
                session_id: sid.into(),
                model: Some("claude-opus-5[1m]".into()),
            }
        );
        assert_eq!(
            events[1],
            AgentEvent::ToolUse {
                id: "toolu_01EmszubqCTMJSoHywVosLrX".into(),
                name: "Bash".into(),
                input: json!({"command": "echo hi", "description": "Print hi"}),
            }
        );
        assert_eq!(
            events[2],
            AgentEvent::ToolResult {
                tool_use_id: "toolu_01EmszubqCTMJSoHywVosLrX".into(),
                output: "hi".into(),
                is_error: false,
            }
        );
        assert_eq!(events[3], AgentEvent::AssistantText { text: "done".into() });
        match &events[4] {
            AgentEvent::Result {
                is_error,
                text,
                duration_ms,
                cost_usd,
                usage,
            } => {
                assert!(!is_error);
                assert_eq!(text.as_deref(), Some("done"));
                assert_eq!(*duration_ms, Some(5269));
                assert!(cost_usd.unwrap() > 0.26);
                assert_eq!(usage.as_ref().unwrap()["output_tokens"], 178);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parse_resume_fixture_keeps_same_session() {
        let (events, state) = parse_all(&fixture("claude_resume.jsonl"));
        assert_eq!(state.session_id.as_deref(), Some("5bce1b59-f2fe-47c3-b821-6a62cf804988"));
        let kinds: Vec<_> = events.iter().map(kind).collect();
        assert_eq!(kinds, ["session_started", "assistant_text", "result"]);
    }

    #[test]
    fn parse_permission_denied_fixture() {
        let (events, _) = parse_all(&fixture("claude_permission_denied.jsonl"));
        let kinds: Vec<_> = events.iter().map(kind).collect();
        assert_eq!(
            kinds,
            ["session_started", "tool_use", "tool_result", "assistant_text", "error", "result"]
        );
        assert!(matches!(
            &events[2],
            AgentEvent::ToolResult { is_error: true, output, .. } if output == "This command requires approval"
        ));
        match &events[4] {
            AgentEvent::Error { message } => {
                assert!(message.contains("1 件"), "{message}");
                assert!(message.contains("Bash: curl -s https://example.com"), "{message}");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(&events[5], AgentEvent::Result { is_error: false, .. }));
    }

    #[test]
    fn parse_resume_not_found_fixture() {
        let (events, state) = parse_all(&fixture("claude_resume_not_found.jsonl"));
        // init が無いのでセッションは確立していない
        assert_eq!(state.session_id, None);
        assert_eq!(events.len(), 1);
        match &events[0] {
            AgentEvent::Result { is_error, text, .. } => {
                assert!(is_error);
                assert!(text.as_deref().unwrap().starts_with("No conversation found with session ID"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn ignores_noise_and_invalid_lines() {
        let mut st = ParseState::default();
        for line in [
            "",
            "not json",
            "[1,2]",
            r#"{"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup"}"#,
            r#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":50}"#,
            r#"{"type":"rate_limit_event"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"","signature":"x"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"  "}]}}"#,
        ] {
            assert!(ClaudeRunner.parse_line(line, &mut st).is_empty(), "{line}");
        }
        assert_eq!(st.session_id, None);
    }

    #[test]
    fn tool_result_array_content_and_thinking() {
        let mut st = ParseState::default();
        let ev = ClaudeRunner.parse_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":[{"type":"text","text":"a"},{"type":"image"},{"type":"text","text":"b"}]}]}}"#,
            &mut st,
        );
        assert_eq!(
            ev,
            vec![AgentEvent::ToolResult {
                tool_use_id: "t1".into(),
                output: "a\n[image]\nb".into(),
                is_error: true,
            }]
        );
        let ev = ClaudeRunner.parse_line(
            r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"考え中"},{"type":"text","text":"答え"}]}}"#,
            &mut st,
        );
        assert_eq!(
            ev,
            vec![
                AgentEvent::Thinking { text: "考え中".into() },
                AgentEvent::AssistantText { text: "答え".into() },
            ]
        );
    }

    #[test]
    fn ignores_sidechain_messages() {
        let mut st = ParseState::default();
        let ev = ClaudeRunner.parse_line(
            r#"{"type":"assistant","parent_tool_use_id":"toolu_parent","message":{"content":[{"type":"text","text":"sub"}]}}"#,
            &mut st,
        );
        assert!(ev.is_empty());
        let ev = ClaudeRunner.parse_line(
            r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"text","text":"main"}]}}"#,
            &mut st,
        );
        assert_eq!(ev, vec![AgentEvent::AssistantText { text: "main".into() }]);
    }

    /// イベントの `type` タグ（snake_case）。
    fn kind(e: &AgentEvent) -> String {
        serde_json::to_value(e).unwrap()["type"].as_str().unwrap().to_string()
    }
}
