//! タスク command（担当: WS-B）。
//!
//! タスク作成 = ブランチ名検証 → worktree パス決定 → `git worktree add` → DB 登録。
//! タスク削除 = 実行中エージェント停止 → (任意) worktree 削除 / ブランチ削除 → DB 削除。

use std::path::PathBuf;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::git;
use crate::models::{new_id, now, CreateTaskRequest, DeleteTaskOptions, Task, UpdateTaskRequest};
use crate::state::{blocking, AppState};

#[tauri::command]
pub async fn list_tasks(state: State<'_, AppState>, project_id: String) -> AppResult<Vec<Task>> {
    state.store.list_tasks(&project_id)
}

#[tauri::command]
pub async fn get_task(state: State<'_, AppState>, task_id: String) -> AppResult<Task> {
    state.store.get_task(&task_id)
}

#[tauri::command]
pub async fn create_task(state: State<'_, AppState>, req: CreateTaskRequest) -> AppResult<Task> {
    let s = state.inner().clone();
    blocking(move || {
        if !git::repo::is_valid_branch_name(&req.branch) {
            return Err(AppError::InvalidInput(format!("ブランチ名が不正です: {}", req.branch)));
        }
        let project = s.store.get_project(&req.project_id)?;
        let repo = PathBuf::from(&project.repo_path);
        let base = req.base_branch.clone().unwrap_or(project.default_branch.clone());
        let wt = git::worktree::worktree_path_for(&repo, &req.branch);
        git::worktree::add_worktree(&s.env, &repo, &wt, &req.branch, &base)?;
        let ts = now();
        let t = Task {
            id: new_id(),
            project_id: project.id,
            title: req.title,
            branch: req.branch,
            base_branch: base,
            worktree_path: wt.display().to_string(),
            agent: req.agent,
            permission: req.permission,
            agent_session_id: None,
            pr_number: None,
            created_at: ts.clone(),
            updated_at: ts,
        };
        s.store.insert_task(&t)?;
        Ok(t)
    })
    .await
}

/// タイトル・エージェント種別・権限の変更。エージェント種別を変えた場合は session をリセットする。
#[tauri::command]
pub async fn update_task(state: State<'_, AppState>, req: UpdateTaskRequest) -> AppResult<Task> {
    let mut t = state.store.get_task(&req.task_id)?;
    if let Some(title) = req.title {
        t.title = title;
    }
    if let Some(agent) = req.agent {
        if agent != t.agent {
            t.agent = agent;
            t.agent_session_id = None;
        }
    }
    if let Some(p) = req.permission {
        t.permission = p;
    }
    t.updated_at = now();
    state.store.update_task(&t)?;
    Ok(t)
}

#[tauri::command]
pub async fn delete_task(state: State<'_, AppState>, task_id: String, options: DeleteTaskOptions) -> AppResult<()> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        let project = s.store.get_project(&t.project_id)?;
        let repo = PathBuf::from(&project.repo_path);
        s.agents.cancel(&t.id)?;
        if options.remove_worktree {
            git::worktree::remove_worktree(&s.env, &repo, &PathBuf::from(&t.worktree_path), options.force)?;
        }
        if options.delete_branch {
            git::worktree::delete_branch(&s.env, &repo, &t.branch, options.force)?;
        }
        s.store.delete_task(&t.id)
    })
    .await
}
