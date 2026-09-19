//! エージェント実行層（ヘッドレス CLI + 正規化イベント）。
//!
//! - `types`   : 正規化イベント・IPC 型（凍結）
//! - `runner`  : `AgentRunner` trait（凍結）
//! - `claude`  : claude 実装（WS-E）
//! - `codex`   : codex 実装（WS-E）
//! - `manager` : プロセス起動・配信（WS-E）

pub mod claude;
pub mod codex;
pub mod manager;
pub mod runner;
pub mod types;

pub use manager::{AgentManager, EventSink, RunContext};
pub use types::*;
