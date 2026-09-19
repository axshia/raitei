//! エージェント実行の正規化イベント型（契約: 凍結）。
//!
//! claude の stream-json / codex exec --json をこの共通型に変換してフロントへ流す。
//! Tauri イベント名は [`AGENT_EVENT`]、ペイロードは [`AgentEventEnvelope`]。
//! TS 側は `src/api/types.ts` の `AgentEvent` / `AgentEventEnvelope`。

use serde::{Deserialize, Serialize};

use crate::models::{AgentKind, Timestamp};

/// Rust → フロントのイベント名。
pub const AGENT_EVENT: &str = "agent://event";

/// 正規化済みエージェントイベント。`type` フィールドでタグ付け（snake_case）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// raitei 側で生成: ユーザーが送ったメッセージ
    UserMessage { text: String },
    /// セッション確立（claude: system/init の session_id, codex: thread.started の thread_id）
    SessionStarted {
        session_id: String,
        model: Option<String>,
    },
    /// アシスタントの本文テキスト（claude: assistant.content[text], codex: item agent_message）
    AssistantText { text: String },
    /// 思考（claude: thinking, codex: reasoning）。空文字は送らない
    Thinking { text: String },
    /// ツール呼び出し開始（codex: command_execution / file_change / mcp_tool_call 等は name に種別を入れる）
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// ツール結果
    ToolResult {
        tool_use_id: String,
        output: String,
        is_error: bool,
    },
    /// ターン完了（claude: type=result, codex: turn.completed / turn.failed）
    Result {
        is_error: bool,
        text: Option<String>,
        duration_ms: Option<u64>,
        cost_usd: Option<f64>,
        usage: Option<serde_json::Value>,
    },
    /// エージェントが報告したエラーや警告（処理は継続しうる）
    Error { message: String },
    /// stderr の行（デバッグ表示用）
    Stderr { line: String },
    /// raitei 側で生成: プロセス終了。必ず各 run の最後に 1 回送る
    RunFinished {
        exit_code: Option<i32>,
        cancelled: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventEnvelope {
    pub task_id: String,
    pub run_id: String,
    pub agent: AgentKind,
    /// タスク内で単調増加する通し番号（履歴の並び順・重複排除に使う）
    pub seq: u64,
    pub timestamp: Timestamp,
    pub event: AgentEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunInfo {
    pub run_id: String,
    pub task_id: String,
    pub agent: AgentKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub task_id: String,
    pub text: String,
}

/// 実行中 run の状態（タブのインジケータ用）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunState {
    pub task_id: String,
    pub running: bool,
    pub run_id: Option<String>,
}
