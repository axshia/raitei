//! エージェント command（担当: WS-E）。
//!
//! イベントは `agent://event`（ペイロード `AgentEventEnvelope`）で非同期に配信される。
//! フロントは起動時に購読し、タブを開いたら `get_agent_history` で過去分を取得して seq で重複排除する。
//!
//! - `send_agent_message`: 即座に `AgentRunInfo` を返す。実行中なら `agent` エラー、
//!   worktree が無ければ `notFound`、CLI が PATH に無ければ `agent` エラー（いずれもイベントは記録しない）
//! - `cancel_agent_run`: 停止を依頼して即座に返る。停止完了は `run_finished { cancelled: true }` で届く
//! - `reset_agent_session`: session_id と履歴を消す。次のメッセージは新しい会話になる

use tauri::State;

use crate::agent::{AgentEventEnvelope, AgentRunInfo, AgentRunState, SendMessageRequest};
use crate::error::{AppError, AppResult};
use crate::state::{blocking, AppState};

#[tauri::command]
pub async fn send_agent_message(state: State<'_, AppState>, req: SendMessageRequest) -> AppResult<AgentRunInfo> {
    if req.text.trim().is_empty() {
        return Err(AppError::InvalidInput("メッセージが空です".into()));
    }
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&req.task_id)?;
        s.agents.start_run(&s.run_context(), &t, req.text)
    })
    .await
}

#[tauri::command]
pub async fn cancel_agent_run(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    state.agents.cancel(&task_id)
}

#[tauri::command]
pub async fn get_agent_history(
    state: State<'_, AppState>,
    task_id: String,
    after_seq: Option<u64>,
) -> AppResult<Vec<AgentEventEnvelope>> {
    let s = state.inner().clone();
    blocking(move || s.store.list_agent_events(&task_id, after_seq)).await
}

#[tauri::command]
pub async fn get_agent_run_state(state: State<'_, AppState>, task_id: String) -> AppResult<AgentRunState> {
    Ok(state.agents.run_state(&task_id))
}

/// 会話をリセット（session_id を破棄し、履歴を消去）。実行中ならエラー。
#[tauri::command]
pub async fn reset_agent_session(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    if state.agents.run_state(&task_id).running {
        return Err(AppError::Agent("実行中はリセットできません。先に停止してください".into()));
    }
    let s = state.inner().clone();
    blocking(move || {
        s.store.set_task_agent_session(&task_id, None)?;
        s.store.clear_agent_events(&task_id)
    })
    .await
}
