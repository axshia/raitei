//! エージェント実行マネージャ（担当: WS-E）。
//!
//! 責務:
//! - [`AgentRunner`] が組み立てたコマンドを tokio で起動（cwd = worktree, PATH = ShellEnv）
//! - stdout を行単位で読み `parse_line` → [`Store::append_agent_event`] で永続化 + seq 採番 → [`EventSink`] へ配信
//! - stderr は `AgentEvent::Stderr` として配信
//! - `SessionStarted` を受けたら `Store::set_task_agent_session` で session_id を保存（次回 resume 用）
//! - 終了時に必ず `RunFinished` を 1 回配信
//! - タスクごとに同時 1 run。実行中に送信されたら `AppError::Agent` を返す
//! - `cancel` で子プロセスを kill し `RunFinished { cancelled: true }`
//!
//! 契約（シグネチャ凍結）: `new` / `start_run` / `cancel` / `run_state`。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::{AppError, AppResult};
use crate::models::{new_id, Task};
use crate::shell_env::ShellEnv;
use crate::store::Store;

#[allow(unused_imports)]
use super::runner::{runner_for, AgentRunner, RunSpec};
use super::types::{AgentEvent, AgentEventEnvelope, AgentRunInfo, AgentRunState};

/// イベント配信先。本番は Tauri の `AppHandle::emit(AGENT_EVENT, ..)`、テストでは Vec に貯める。
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, envelope: &AgentEventEnvelope);
}

/// run 実行に必要な依存。
#[derive(Clone)]
pub struct RunContext {
    pub env: ShellEnv,
    pub store: Arc<Store>,
    pub sink: Arc<dyn EventSink>,
}

#[derive(Default)]
pub struct AgentManager {
    /// task_id → 実行中 run_id（WS-E が子プロセスハンドル等を持つ構造に拡張してよい）
    running: Mutex<HashMap<String, String>>,
}

impl AgentManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// タスクの worktree でエージェントに 1 ターン実行させる。即座に返り、イベントは非同期で配信される。
    ///
    /// 仮実装: UserMessage → Error("未実装") → RunFinished を同期で配信するだけ。
    pub fn start_run(&self, ctx: &RunContext, task: &Task, prompt: String) -> AppResult<AgentRunInfo> {
        {
            let running = self.running.lock().unwrap();
            if running.contains_key(&task.id) {
                return Err(AppError::Agent("このタスクではエージェントが実行中です".into()));
            }
        }
        let run_id = new_id();
        let emit = |event: AgentEvent| -> AppResult<()> {
            let env = ctx.store.append_agent_event(&task.id, &run_id, task.agent, event)?;
            ctx.sink.emit(&env);
            Ok(())
        };
        emit(AgentEvent::UserMessage { text: prompt })?;
        emit(AgentEvent::Error {
            message: "エージェント実行は未実装です（WS-E）".into(),
        })?;
        emit(AgentEvent::RunFinished {
            exit_code: None,
            cancelled: false,
        })?;
        Ok(AgentRunInfo {
            run_id,
            task_id: task.id.clone(),
            agent: task.agent,
        })
    }

    /// 実行中の run を中断する。実行中でなければ何もしない。
    pub fn cancel(&self, task_id: &str) -> AppResult<()> {
        self.running.lock().unwrap().remove(task_id);
        Ok(())
    }

    pub fn run_state(&self, task_id: &str) -> AgentRunState {
        let running = self.running.lock().unwrap();
        let run_id = running.get(task_id).cloned();
        AgentRunState {
            task_id: task_id.to_string(),
            running: run_id.is_some(),
            run_id,
        }
    }
}
