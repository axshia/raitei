//! worktree 操作（担当: WS-C）。
//!
//! worktree の配置規約: `<repo の親>/<repo 名>.worktrees/<ブランチ名の / を - に置換>`
//! 例: `/src/app` + `feat/login` → `/src/app.worktrees/feat-login`

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::shell_env::{run, ShellEnv};

use super::types::WorktreeInfo;

/// タスクの worktree パスを決める（純粋関数）。
pub fn worktree_path_for(repo: &Path, branch: &str) -> PathBuf {
    let name = repo
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = repo.parent().unwrap_or(repo);
    parent
        .join(format!("{name}.worktrees"))
        .join(branch.replace('/', "-"))
}

/// `git worktree list --porcelain` 一覧。
pub fn list_worktrees(env: &ShellEnv, repo: &Path) -> AppResult<Vec<WorktreeInfo>> {
    let out = super::git(env, repo, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_porcelain(&out))
}

/// `git worktree add -b <branch> <path> <base>`（branch が既存なら `-b` なしで checkout）。
pub fn add_worktree(
    env: &ShellEnv,
    repo: &Path,
    path: &Path,
    branch: &str,
    base: &str,
) -> AppResult<()> {
    super::repo::validate_branch(branch)?;
    super::repo::validate_branch(base)?;
    let path = path.to_string_lossy();
    if super::repo::branch_exists(env, repo, branch)? {
        super::git(env, repo, &["worktree", "add", "--", &path, branch])?;
    } else {
        super::git(
            env,
            repo,
            &["worktree", "add", "-b", branch, "--", &path, base],
        )?;
    }
    Ok(())
}

/// 未コミット変更のある worktree は force 指定時だけ削除する。
pub fn remove_worktree(env: &ShellEnv, repo: &Path, path: &Path, force: bool) -> AppResult<()> {
    let path = path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.extend(["--", &path]);
    super::git(env, repo, &args)?;
    super::git(env, repo, &["worktree", "prune"])?;
    Ok(())
}

/// `git branch -d` が受け付けるか（マージ済みか）を、ブランチを消さずに調べる。
///
/// git と同じく、upstream があればそこへ、無ければリポジトリの HEAD へマージ済みかを見る。
pub fn is_branch_merged(env: &ShellEnv, repo: &Path, branch: &str) -> AppResult<bool> {
    super::repo::validate_branch(branch)?;
    let upstream = run(
        env,
        "git",
        &["rev-parse", "--verify", "--quiet", &format!("{branch}@{{upstream}}")],
        repo,
    )?;
    let reference = if upstream.success() {
        upstream.stdout.trim().to_string()
    } else {
        "HEAD".to_string()
    };
    let local = format!("refs/heads/{branch}");
    let out = run(
        env,
        "git",
        &["merge-base", "--is-ancestor", &local, &reference],
        repo,
    )?;
    match out.status {
        0 => Ok(true),
        1 => Ok(false),
        _ => Err(AppError::Git(format!(
            "ブランチ {branch} のマージ状態を確認できません: {}",
            out.stderr.trim()
        ))),
    }
}

pub fn delete_branch(env: &ShellEnv, repo: &Path, branch: &str, force: bool) -> AppResult<()> {
    super::repo::validate_branch(branch)?;
    super::git(
        env,
        repo,
        &["branch", if force { "-D" } else { "-d" }, "--", branch],
    )?;
    Ok(())
}

/// porcelain 出力のパース（純粋関数）。
///
/// Git の引用付きパスと locked/prunable の理由付き行にも対応する。
pub fn parse_worktree_porcelain(s: &str) -> Vec<WorktreeInfo> {
    let mut out: Vec<WorktreeInfo> = Vec::new();
    for block in s.split("\n\n") {
        let mut wt = WorktreeInfo::default();
        let mut seen = false;
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                wt.path = super::parse_quoted_path(p);
                seen = true;
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                wt.head = Some(h.to_string());
            } else if let Some(b) = line.strip_prefix("branch ") {
                wt.branch = Some(b.trim_start_matches("refs/heads/").to_string());
            } else if line == "bare" {
                wt.is_bare = true;
            } else if line == "detached" {
                wt.is_detached = true;
            } else if line == "locked" || line.starts_with("locked ") {
                wt.locked = true;
            } else if line == "prunable" || line.starts_with("prunable ") {
                wt.prunable = true;
            }
        }
        if seen {
            wt.is_main = out.is_empty();
            out.push(wt);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_convention() {
        assert_eq!(
            worktree_path_for(Path::new("/src/app"), "feat/login"),
            PathBuf::from("/src/app.worktrees/feat-login")
        );
    }

    #[test]
    fn parse_basic() {
        let s =
            "worktree /a\nHEAD 111\nbranch refs/heads/main\n\nworktree /b\nHEAD 222\ndetached\n";
        let v = parse_worktree_porcelain(s);
        assert_eq!(v.len(), 2);
        assert!(v[0].is_main);
        assert_eq!(v[0].branch.as_deref(), Some("main"));
        assert!(v[1].is_detached);
    }
}
