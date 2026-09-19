//! PR command（担当: WS-D）。

use std::path::PathBuf;

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::git;
use crate::github::{self, CreatePrRequest, MergePrRequest, PullRequestStatus};
use crate::state::{blocking, AppState};

/// タスクのブランチに紐づく PR の状態。PR が無ければ None。
/// 取得できた場合は Task.pr_number を更新する。
#[tauri::command]
pub async fn get_pull_request(state: State<'_, AppState>, task_id: String) -> AppResult<Option<PullRequestStatus>> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&task_id)?;
        let pr = github::gh::pr_for_branch(&s.env, &PathBuf::from(&t.worktree_path), &t.branch)?;
        if let Some(p) = &pr {
            if t.pr_number != Some(p.number) {
                s.store.set_task_pr_number(&t.id, Some(p.number))?;
            }
        }
        Ok(pr)
    })
    .await
}

/// ブランチを push（upstream 設定）してから PR を作成する。
#[tauri::command]
pub async fn create_pull_request(state: State<'_, AppState>, req: CreatePrRequest) -> AppResult<PullRequestStatus> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&req.task_id)?;
        let wt = PathBuf::from(&t.worktree_path);
        let base = req.base.clone().unwrap_or(t.base_branch.clone());
        git::repo::push(&s.env, &wt, &t.branch, true)?;
        let pr = github::gh::create_pr(&s.env, &wt, &t.branch, &base, &req.title, &req.body, req.draft)?;
        s.store.set_task_pr_number(&t.id, Some(pr.number))?;
        Ok(pr)
    })
    .await
}

/// PR をマージし、マージ後の状態を返す。
#[tauri::command]
pub async fn merge_pull_request(state: State<'_, AppState>, req: MergePrRequest) -> AppResult<PullRequestStatus> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&req.task_id)?;
        let number = t
            .pr_number
            .ok_or_else(|| AppError::InvalidInput("このタスクには PR がありません".into()))?;
        let wt = PathBuf::from(&t.worktree_path);
        github::gh::merge_pr(&s.env, &wt, number, req.method)?;
        if req.delete_remote_branch {
            // WS-D: `git push origin --delete <branch>` を実装（失敗は警告扱いでよい）
        }
        github::gh::pr_view(&s.env, &wt, number)
    })
    .await
}
