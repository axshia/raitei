//! worktree / 状態 / コンフリクト解消 command（担当: WS-C）。

use std::path::PathBuf;

use tauri::State;

use crate::agent::AgentRunInfo;
use crate::error::{AppError, AppResult};
use crate::git::{self, ConflictFileContent, ConflictResolution, ConflictState, GitStatus, WorktreeInfo};
use crate::state::{blocking, AppState};

/// git が認識する全 worktree（raitei 管理外も含む）。raitei のタスクに紐づくものは task_id を埋める。
#[tauri::command]
pub async fn list_worktrees(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<WorktreeInfo>> {
    let s = state.inner().clone();
    blocking(move || {
        let p = s.store.get_project(&project_id)?;
        let tasks = s.store.list_tasks(&project_id)?;
        let mut v = git::worktree::list_worktrees(&s.env, &PathBuf::from(&p.repo_path))?;
        for w in v.iter_mut() {
            w.task_id = tasks.iter().find(|t| t.worktree_path == w.path).map(|t| t.id.clone());
        }
        Ok(v)
    })
    .await
}

#[tauri::command]
pub async fn get_git_status(state: State<'_, AppState>, task_id: String) -> AppResult<GitStatus> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        git::status::status(&s.env, &PathBuf::from(&t.worktree_path))
    })
    .await
}

/// base ブランチをタスクブランチへ取り込む（コンフリクト解消フローの開始）。
#[tauri::command]
pub async fn start_base_merge(state: State<'_, AppState>, task_id: String) -> AppResult<ConflictState> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        git::conflict::merge_base_into(&s.env, &PathBuf::from(&t.worktree_path), &t.base_branch)
    })
    .await
}

#[tauri::command]
pub async fn get_conflict_state(state: State<'_, AppState>, task_id: String) -> AppResult<ConflictState> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        git::conflict::conflict_state(&s.env, &PathBuf::from(&t.worktree_path))
    })
    .await
}

#[tauri::command]
pub async fn read_conflict_file(
    state: State<'_, AppState>,
    task_id: String,
    path: String,
) -> AppResult<ConflictFileContent> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        git::conflict::read_conflict_file(&s.env, &PathBuf::from(&t.worktree_path), &path)
    })
    .await
}

#[tauri::command]
pub async fn resolve_conflict_file(
    state: State<'_, AppState>,
    task_id: String,
    path: String,
    resolution: ConflictResolution,
) -> AppResult<ConflictState> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        git::conflict::resolve_file(&s.env, &PathBuf::from(&t.worktree_path), &path, resolution)
    })
    .await
}

#[tauri::command]
pub async fn abort_base_merge(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        git::conflict::abort_merge(&s.env, &PathBuf::from(&t.worktree_path))
    })
    .await
}

/// マージコミットを作成し、`push` が true なら origin へ push する。
#[tauri::command]
pub async fn complete_base_merge(state: State<'_, AppState>, task_id: String, push: bool) -> AppResult<ConflictState> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        let wt = PathBuf::from(&t.worktree_path);
        git::conflict::commit_merge(&s.env, &wt)?;
        if push {
            git::repo::push(&s.env, &wt, &t.branch, true)?;
        }
        git::conflict::conflict_state(&s.env, &wt)
    })
    .await
}

/// 現在の競合ファイル一覧を元にプロンプトを作り、タスクのエージェントに解消を依頼する。
#[tauri::command]
pub async fn request_agent_conflict_resolution(state: State<'_, AppState>, task_id: String) -> AppResult<AgentRunInfo> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        let cs = git::conflict::conflict_state(&s.env, &PathBuf::from(&t.worktree_path))?;
        if cs.files.is_empty() {
            return Err(AppError::InvalidInput("未解決のコンフリクトはありません".into()));
        }
        let base_ref = cs.base_ref.clone().unwrap_or_else(|| t.base_branch.clone());
        let prompt = git::conflict::build_agent_prompt(&base_ref, &cs.files);
        s.agents.start_run(&s.run_context(), &t, prompt)
    })
    .await
}
