//! アプリ共有状態（契約: 凍結）。

use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use crate::agent::{AgentEventEnvelope, AgentManager, EventSink, RunContext, AGENT_EVENT};
use crate::error::{AppError, AppResult};
use crate::shell_env::ShellEnv;
use crate::store::Store;

#[derive(Clone)]
pub struct AppState {
    pub env: ShellEnv,
    pub store: Arc<Store>,
    pub agents: Arc<AgentManager>,
    pub sink: Arc<dyn EventSink>,
}

impl AppState {
    pub fn run_context(&self) -> RunContext {
        RunContext {
            env: self.env.clone(),
            store: self.store.clone(),
            sink: self.sink.clone(),
        }
    }
}

/// Tauri のイベントとして配信する EventSink。
pub struct TauriSink(pub AppHandle);

impl EventSink for TauriSink {
    fn emit(&self, envelope: &AgentEventEnvelope) {
        let _ = self.0.emit(AGENT_EVENT, envelope);
    }
}

/// ブロッキング処理（git / gh の同期実行など）を専用スレッドで実行する。
/// **全 command はこれを通して外部コマンドを呼ぶこと**（メインスレッド・async ワーカーを塞がないため）。
pub async fn blocking<T, F>(f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Command(format!("バックグラウンド実行に失敗: {e}")))?
}
