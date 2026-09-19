//! エージェント実行層の抽象（契約: 凍結）。
//!
//! 1 ユーザーメッセージ = 1 子プロセス（ターン）という実行モデル。
//! 継続は各 CLI の resume 機能（claude `--resume <session_id>` / codex `exec resume <thread_id>`）で行う。
//!
//! Runner は「コマンド組み立て」と「JSONL 1 行 → 正規化イベント」の純粋関数だけを持ち、
//! プロセス起動・stdout 読み取り・イベント配信は [`super::manager::AgentManager`] が共通で担う。
//! これにより Runner はプロセス無しでユニットテストできる。

use std::path::PathBuf;

use crate::models::{AgentKind, PermissionLevel};

use super::types::AgentEvent;

/// 1 ターン実行の入力。
#[derive(Debug, Clone)]
pub struct RunSpec {
    /// 作業ディレクトリ（タスクの worktree）
    pub cwd: PathBuf,
    /// ユーザープロンプト
    pub prompt: String,
    /// 継続するセッション ID（なければ新規）
    pub resume_session_id: Option<String>,
    pub permission: PermissionLevel,
    /// モデル指定（任意）
    pub model: Option<String>,
}

/// 起動するコマンド。`program` は名前（`claude` / `codex`）で、ShellEnv が PATH から解決する。
#[derive(Debug, Clone, PartialEq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    /// Some の場合は stdin にこの文字列を書き込んで閉じる。None なら stdin は null。
    pub stdin: Option<String>,
}

/// パーサの状態（行をまたいだ情報を保持）。
#[derive(Debug, Clone, Default)]
pub struct ParseState {
    /// 検出したセッション ID（claude session_id / codex thread_id）
    pub session_id: Option<String>,
    /// 最後の assistant テキスト（codex の turn.completed に本文が無いため Result.text 用に保持）
    pub last_text: Option<String>,
}

pub trait AgentRunner: Send + Sync {
    fn kind(&self) -> AgentKind;

    /// 1 ターン分のコマンドを組み立てる。
    fn build_command(&self, spec: &RunSpec) -> CommandSpec;

    /// stdout の 1 行（JSONL）を 0 個以上の正規化イベントに変換する。
    /// JSON でない行・未知イベントは無視してよい（空 Vec）。
    fn parse_line(&self, line: &str, state: &mut ParseState) -> Vec<AgentEvent>;
}

/// 種別から Runner を得る。
pub fn runner_for(kind: AgentKind) -> Box<dyn AgentRunner> {
    match kind {
        AgentKind::Claude => Box::new(super::claude::ClaudeRunner),
        AgentKind::Codex => Box::new(super::codex::CodexRunner),
    }
}
