//! worktree 操作（担当: WS-C）。
//!
//! worktree の配置規約: `<repo の親>/<repo 名>.worktrees/<ブランチ名の / を - に置換>`
//! 例: `/src/app` + `feat/login` → `/src/app.worktrees/feat-login`

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::shell_env::ShellEnv;

use super::types::WorktreeInfo;

/// タスクの worktree パスを決める（純粋関数）。
pub fn worktree_path_for(repo: &Path, branch: &str) -> PathBuf {
    let name = repo.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let parent = repo.parent().unwrap_or(repo);
    parent.join(format!("{name}.worktrees")).join(branch.replace('/', "-"))
}

/// `git worktree list --porcelain` 一覧。
pub fn list_worktrees(env: &ShellEnv, repo: &Path) -> AppResult<Vec<WorktreeInfo>> {
    let out = super::git(env, repo, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_porcelain(&out))
}

/// `git worktree add -b <branch> <path> <base>`（branch が既存なら `-b` なしで checkout）。
pub fn add_worktree(_env: &ShellEnv, _repo: &Path, _path: &Path, _branch: &str, _base: &str) -> AppResult<()> {
    Err(AppError::NotImplemented("git::worktree::add_worktree"))
}

/// `git worktree remove [--force] <path>` → `git worktree prune`。
pub fn remove_worktree(_env: &ShellEnv, _repo: &Path, _path: &Path, _force: bool) -> AppResult<()> {
    Err(AppError::NotImplemented("git::worktree::remove_worktree"))
}

/// `git branch -d|-D <branch>`。
pub fn delete_branch(_env: &ShellEnv, _repo: &Path, _branch: &str, _force: bool) -> AppResult<()> {
    Err(AppError::NotImplemented("git::worktree::delete_branch"))
}

/// porcelain 出力のパース（純粋関数）。
///
/// 仮実装: `worktree` / `HEAD` / `branch` / `bare` / `detached` のみ対応。WS-C が locked/prunable を含め完成させテストを書く。
pub fn parse_worktree_porcelain(s: &str) -> Vec<WorktreeInfo> {
    let mut out: Vec<WorktreeInfo> = Vec::new();
    for block in s.split("\n\n") {
        let mut wt = WorktreeInfo::default();
        let mut seen = false;
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                wt.path = p.to_string();
                seen = true;
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                wt.head = Some(h.to_string());
            } else if let Some(b) = line.strip_prefix("branch ") {
                wt.branch = Some(b.trim_start_matches("refs/heads/").to_string());
            } else if line == "bare" {
                wt.is_bare = true;
            } else if line == "detached" {
                wt.is_detached = true;
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
        let s = "worktree /a\nHEAD 111\nbranch refs/heads/main\n\nworktree /b\nHEAD 222\ndetached\n";
        let v = parse_worktree_porcelain(s);
        assert_eq!(v.len(), 2);
        assert!(v[0].is_main);
        assert_eq!(v[0].branch.as_deref(), Some("main"));
        assert!(v[1].is_detached);
    }
}
