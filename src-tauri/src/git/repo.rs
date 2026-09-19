//! リポジトリ操作（担当: WS-C）。

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::shell_env::ShellEnv;

use super::git;

/// `path` が git 作業ツリー内ならトップレベルの絶対パスを返す。
pub fn repo_root(env: &ShellEnv, path: &Path) -> AppResult<PathBuf> {
    let out = git(env, path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim()))
}

/// 新規ディレクトリに `git init -b main` し、空の初回コミットを作る。
/// 既に存在して空でないディレクトリなら `AppError::InvalidInput`。
pub fn init_repo(_env: &ShellEnv, _path: &Path, _default_branch: &str) -> AppResult<()> {
    Err(AppError::NotImplemented("git::repo::init_repo"))
}

/// 既定ブランチを推定する: `origin/HEAD` → `main` / `master` の存在 → 現在のブランチ。
pub fn default_branch(env: &ShellEnv, repo: &Path) -> AppResult<String> {
    current_branch(env, repo).map(|b| b.unwrap_or_else(|| "main".into()))
}

/// 現在のブランチ。detached なら None。
pub fn current_branch(env: &ShellEnv, repo: &Path) -> AppResult<Option<String>> {
    let out = git(env, repo, &["branch", "--show-current"])?;
    let b = out.trim();
    Ok(if b.is_empty() { None } else { Some(b.to_string()) })
}

/// `origin` リモートがあるか。
pub fn has_origin(_env: &ShellEnv, _repo: &Path) -> AppResult<bool> {
    Err(AppError::NotImplemented("git::repo::has_origin"))
}

/// `git fetch origin <branch>`。
pub fn fetch(_env: &ShellEnv, _repo: &Path, _branch: &str) -> AppResult<()> {
    Err(AppError::NotImplemented("git::repo::fetch"))
}

/// `git push [-u] origin <branch>`。
pub fn push(_env: &ShellEnv, _worktree: &Path, _branch: &str, _set_upstream: bool) -> AppResult<()> {
    Err(AppError::NotImplemented("git::repo::push"))
}

/// ブランチ名として妥当か（`git check-ref-format --branch` 相当の簡易検証）。
pub fn is_valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.contains("..")
        && !name.contains(char::is_whitespace)
        && !name.ends_with('/')
        && !name.ends_with(".lock")
}
