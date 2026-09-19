//! エージェント command（担当: WS-E）。
//!
//! イベントは `agent://event`（ペイロード `AgentEventEnvelope`）で非同期に配信される。
//! フロントは起動時に購読し、タブを開いたら `get_agent_history` で過去分を取得して seq で重複排除する。

use tauri::State;

use crate::agent::{AgentEventEnvelope, AgentRunInfo, AgentRunState, SendMessageRequest};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[tauri::command]
pub async fn send_agent_message(state: State<'_, AppState>, req: SendMessageRequest) -> AppResult<AgentRunInfo> {
    if req.text.trim().is_empty() {
        return Err(AppError::InvalidInput("メッセージが空です".into()));
    }
    let t = state.store.get_task(&req.task_id)?;
    state.agents.start_run(&state.run_context(), &t, req.text)
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
    state.store.list_agent_events(&task_id, after_seq)
}

#[tauri::command]
pub async fn get_agent_run_state(state: State<'_, AppState>, task_id: String) -> AppResult<AgentRunState> {
    Ok(state.agents.run_state(&task_id))
}

/// 会話をリセット（session_id を破棄し、履歴を消去）。実行中ならエラー。
#[tauri::command]
pub async fn reset_agent_session(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    if state.agents.run_state(&task_id).running {
        return Err(AppError::Agent("実行中はリセットできません".into()));
    }
    state.store.set_task_agent_session(&task_id, None)?;
    state.store.clear_agent_events(&task_id)
}
