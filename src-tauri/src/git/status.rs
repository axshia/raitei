//! 作業ツリー状態（担当: WS-C）。

use std::path::Path;

use crate::error::AppResult;
use crate::shell_env::ShellEnv;

use super::types::GitStatus;

/// `git status --porcelain=v2 --branch` を実行してパースする。
pub fn status(env: &ShellEnv, worktree: &Path) -> AppResult<GitStatus> {
    let out = super::git(env, worktree, &["status", "--porcelain=v2", "--branch"])?;
    let mut st = parse_status_porcelain_v2(&out);
    st.merge_in_progress = super::git(env, worktree, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok();
    Ok(st)
}

/// porcelain v2 のパース（純粋関数）。
///
/// 仮実装: ヘッダ（branch.head / branch.upstream / branch.ab）のみ。WS-C がエントリ行（1/2/u/?）を実装しテストを書く。
pub fn parse_status_porcelain_v2(s: &str) -> GitStatus {
    let mut st = GitStatus::default();
    for line in s.lines() {
        if let Some(h) = line.strip_prefix("# branch.head ") {
            st.branch = (h != "(detached)").then(|| h.to_string());
        } else if let Some(u) = line.strip_prefix("# branch.upstream ") {
            st.upstream = Some(u.to_string());
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let mut it = ab.split_whitespace();
            st.ahead = it.next().and_then(|a| a.trim_start_matches('+').parse().ok()).unwrap_or(0);
            st.behind = it.next().and_then(|b| b.trim_start_matches('-').parse().ok()).unwrap_or(0);
        }
    }
    st
}
