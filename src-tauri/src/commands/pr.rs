//! PR command（担当: WS-D）。

use std::path::{Path, PathBuf};

use tauri::State;

use crate::error::{AppError, AppResult};
use crate::git;
use crate::github::{self, CreatePrRequest, MergePrRequest, PrState, PullRequestStatus};
use crate::shell_env::ShellEnv;
use crate::state::{blocking, AppState};

/// タスクのブランチに紐づく PR の状態。PR が無ければ None。
/// 取得できた場合は Task.pr_number を更新する。
#[tauri::command]
pub async fn get_pull_request(
    state: State<'_, AppState>,
    task_id: String,
) -> AppResult<Option<PullRequestStatus>> {
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
pub async fn create_pull_request(
    state: State<'_, AppState>,
    req: CreatePrRequest,
) -> AppResult<PullRequestStatus> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&req.task_id)?;
        let wt = PathBuf::from(&t.worktree_path);
        let base = req.base.clone().unwrap_or(t.base_branch.clone());
        github::gh::validate_create_input(&t.branch, &base, &req.title, &req.body)?;
        git::repo::push(&s.env, &wt, &t.branch, true)?;
        let pr = github::gh::create_pr(
            &s.env, &wt, &t.branch, &base, &req.title, &req.body, req.draft,
        )?;
        s.store.set_task_pr_number(&t.id, Some(pr.number))?;
        Ok(pr)
    })
    .await
}

/// PR をマージし、マージ後の状態を返す。
#[tauri::command]
pub async fn merge_pull_request(
    state: State<'_, AppState>,
    req: MergePrRequest,
) -> AppResult<PullRequestStatus> {
    let s = state.inner().clone();
    blocking(move || {
        let t = s.store.get_task(&req.task_id)?;
        let number = t
            .pr_number
            .ok_or_else(|| AppError::InvalidInput("このタスクには PR がありません".into()))?;
        let wt = PathBuf::from(&t.worktree_path);
        github::gh::merge_pr(&s.env, &wt, number, req.method)?;
        let pr = github::gh::pr_view(&s.env, &wt, number)?;
        delete_merged_remote_branch(&s.env, &wt, &t.branch, &pr, req.delete_remote_branch)?;
        Ok(pr)
    })
    .await
}

/// merge queue への登録は完了ではない。MERGED を確認できた場合だけ origin を削除する。
fn delete_merged_remote_branch(
    env: &ShellEnv,
    worktree: &Path,
    branch: &str,
    pr: &PullRequestStatus,
    requested: bool,
) -> AppResult<()> {
    if !requested || pr.state != PrState::Merged {
        return Ok(());
    }
    // Stored PR numbers must not cause deletion of a different task/base branch.
    if branch != pr.head_branch
        || branch == pr.base_branch
        || !git::repo::is_valid_branch_name(branch)
    {
        return Err(AppError::InvalidInput(
            "PR はマージ済みですが、削除対象のリモートブランチが一致しません".into(),
        ));
    }
    git::git(env, worktree, &["push", "origin", "--delete", branch])
        .map(|_| ())
        .map_err(|e| {
            AppError::Git(format!(
                "PR #{} はマージ済みですが、リモートブランチを削除できません: {e}",
                pr.number
            ))
        })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::github::{parse::parse_pr_view, test_support::FakeCli};

    fn sample() -> PullRequestStatus {
        parse_pr_view(include_str!("../github/fixtures/gh_pr_view_mixed.json")).unwrap()
    }

    #[test]
    fn deletes_only_remote_after_confirmed_merge_when_requested() {
        let cli = FakeCli::new();
        let mut pr = sample();
        for state in [PrState::Open, PrState::Closed] {
            pr.state = state;
            delete_merged_remote_branch(&cli.env, cli.repo(), &pr.head_branch, &pr, true).unwrap();
        }
        pr.state = PrState::Merged;
        delete_merged_remote_branch(&cli.env, cli.repo(), &pr.head_branch, &pr, false).unwrap();
        assert!(cli.calls().is_empty());
        delete_merged_remote_branch(&cli.env, cli.repo(), &pr.head_branch, &pr, true).unwrap();
        assert_eq!(
            cli.calls(),
            vec![vec!["push", "origin", "--delete", "feature/pr-status"]]
        );
    }

    #[test]
    fn deletion_failure_reports_that_merge_already_succeeded() {
        let cli = FakeCli::new();
        cli.fail("origin", "permission denied");
        let mut pr = sample();
        pr.state = PrState::Merged;
        let error = delete_merged_remote_branch(&cli.env, cli.repo(), &pr.head_branch, &pr, true)
            .unwrap_err();
        assert_eq!(error.kind(), "git");
        assert!(error.to_string().contains("#42 はマージ済み"));
        assert!(error.to_string().contains("permission denied"));
    }

    #[test]
    fn mismatched_or_base_branch_is_not_deleted() {
        let cli = FakeCli::new();
        let mut pr = sample();
        pr.state = PrState::Merged;
        assert!(delete_merged_remote_branch(&cli.env, cli.repo(), "different", &pr, true).is_err());
        pr.head_branch = pr.base_branch.clone();
        assert!(
            delete_merged_remote_branch(&cli.env, cli.repo(), &pr.head_branch, &pr, true).is_err()
        );
        assert!(cli.calls().is_empty());
    }
}
