//! リポジトリ操作（担当: WS-C）。

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::shell_env::{run, ShellEnv};

use super::git;

/// `path` が git 作業ツリー内ならトップレベルの絶対パスを返す。
pub fn repo_root(env: &ShellEnv, path: &Path) -> AppResult<PathBuf> {
    let out = git(env, path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim_end_matches('\n')))
}

/// 空のディレクトリに指定ブランチと初回コミットを作る。既存データは変更しない。
pub fn init_repo(env: &ShellEnv, path: &Path, default_branch: &str) -> AppResult<()> {
    validate_branch(default_branch)?;
    if path.exists() && (!path.is_dir() || std::fs::read_dir(path)?.next().is_some()) {
        return Err(AppError::InvalidInput(
            "作成先は空のディレクトリを指定してください".into(),
        ));
    }
    std::fs::create_dir_all(path)?;
    git(env, path, &["init", "-b", default_branch])?;
    git(
        env,
        path,
        &["commit", "--allow-empty", "-m", "Initial commit"],
    )?;
    Ok(())
}

/// origin/HEAD → main / master → 現在のブランチ。
pub fn default_branch(env: &ShellEnv, repo: &Path) -> AppResult<String> {
    repo_root(env, repo)?;
    let remote = run(
        env,
        "git",
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
        repo,
    )?;
    if remote.success() {
        if let Some(branch) = remote.stdout.trim().strip_prefix("refs/remotes/origin/") {
            return Ok(branch.to_string());
        }
    }
    for branch in ["main", "master"] {
        if branch_exists(env, repo, branch)? {
            return Ok(branch.to_string());
        }
    }
    current_branch(env, repo)?.ok_or_else(|| {
        AppError::InvalidInput("既定ブランチを判定できません（detached HEAD）".into())
    })
}

pub fn current_branch(env: &ShellEnv, repo: &Path) -> AppResult<Option<String>> {
    let out = git(env, repo, &["branch", "--show-current"])?;
    let b = out.trim();
    Ok(if b.is_empty() {
        None
    } else {
        Some(b.to_string())
    })
}

pub fn has_origin(env: &ShellEnv, repo: &Path) -> AppResult<bool> {
    repo_root(env, repo)?;
    let out = run(env, "git", &["remote", "get-url", "origin"], repo)?;
    if out.success() {
        return Ok(true);
    }
    // Distinguish an absent origin from an invalid repository/configuration.
    let remotes = git(env, repo, &["remote"])?;
    if remotes.lines().any(|r| r == "origin") {
        return Err(AppError::Git(out.stderr));
    }
    Ok(false)
}

pub fn fetch(env: &ShellEnv, repo: &Path, branch: &str) -> AppResult<()> {
    validate_branch(branch)?;
    // An explicit destination also supports clones with a restricted fetch refspec.
    let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    git(env, repo, &["fetch", "origin", &refspec])?;
    Ok(())
}

pub fn push(env: &ShellEnv, worktree: &Path, branch: &str, set_upstream: bool) -> AppResult<()> {
    validate_branch(branch)?;
    let mut args = vec!["push"];
    if set_upstream {
        args.push("-u");
    }
    // Fully qualify the ref to avoid ambiguity with a tag of the same name.
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    args.extend(["origin", &refspec]);
    git(env, worktree, &args)?;
    Ok(())
}

pub(crate) fn branch_exists(env: &ShellEnv, repo: &Path, branch: &str) -> AppResult<bool> {
    let reference = format!("refs/heads/{branch}");
    let out = run(
        env,
        "git",
        &["show-ref", "--verify", "--quiet", &reference],
        repo,
    )?;
    match out.status {
        0 => Ok(true),
        1 => Ok(false),
        _ => Err(AppError::Git(out.stderr)),
    }
}

pub(crate) fn validate_branch(name: &str) -> AppResult<()> {
    if is_valid_branch_name(name) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(format!(
            "ブランチ名が不正です: {name}"
        )))
    }
}

/// シェル/オプションや refspec ではなく、リテラルのブランチ名だけを許可する。
pub fn is_valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name != "HEAD"
        && name != "@"
        && !name.starts_with('-')
        && !name.ends_with('.')
        && !name.contains("..")
        && !name.contains("@{")
        && !name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
        && name
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}
