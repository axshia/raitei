//! gh コマンド実行（担当: WS-D）。

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::shell_env::{run, ShellEnv};

use super::types::{MergeMethod, PullRequestStatus};

/// `gh auth status` が成功するか。
pub fn is_authenticated(env: &ShellEnv) -> bool {
    run(env, "gh", &["auth", "status"], Path::new("/"))
        .map(|o| o.success())
        .unwrap_or(false)
}

/// ブランチに紐づく PR（open 優先、なければ最新）。無ければ None。
/// 実装: `gh pr view <branch> --json <PR_JSON_FIELDS>`（"no pull requests found" は None 扱い）。
pub fn pr_for_branch(_env: &ShellEnv, _repo: &Path, _branch: &str) -> AppResult<Option<PullRequestStatus>> {
    Ok(None)
}

/// PR 番号で取得。
pub fn pr_view(_env: &ShellEnv, _repo: &Path, _number: u64) -> AppResult<PullRequestStatus> {
    Err(AppError::NotImplemented("github::gh::pr_view"))
}

/// `gh pr create --head <head> --base <base> --title --body [--draft]` を実行し、作成した PR を返す。
/// push 済みであること（コマンド層が事前に `git::repo::push` する）。
pub fn create_pr(
    _env: &ShellEnv,
    _repo: &Path,
    _head: &str,
    _base: &str,
    _title: &str,
    _body: &str,
    _draft: bool,
) -> AppResult<PullRequestStatus> {
    Err(AppError::NotImplemented("github::gh::create_pr"))
}

/// `gh pr merge <number> --merge|--squash|--rebase`。
/// `--delete-branch` は worktree のローカルブランチ操作を伴うため使わない。
/// リモートブランチ削除は `git push origin --delete <branch>` で行う。
pub fn merge_pr(_env: &ShellEnv, _repo: &Path, _number: u64, _method: MergeMethod) -> AppResult<()> {
    Err(AppError::NotImplemented("github::gh::merge_pr"))
}
