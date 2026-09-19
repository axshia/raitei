//! 作業ツリー状態（担当: WS-C）。

use std::path::Path;

use crate::error::AppResult;
use crate::shell_env::ShellEnv;

use super::types::{FileChange, GitStatus};

/// `git status --porcelain=v2 --branch` を実行してパースする。
pub fn status(env: &ShellEnv, worktree: &Path) -> AppResult<GitStatus> {
    let out = super::git(
        env,
        worktree,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    let mut st = parse_status_porcelain_v2(&out);
    st.merge_in_progress = super::git(
        env,
        worktree,
        &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
    )
    .is_ok();
    Ok(st)
}

/// porcelain v2 のパース（純粋関数）。
///
/// v2 のドットを契約の空白 XY ステータスに正規化する。リネームは移動先を返す。
pub fn parse_status_porcelain_v2(s: &str) -> GitStatus {
    let mut st = GitStatus::default();
    for line in s.lines() {
        if let Some(h) = line.strip_prefix("# branch.head ") {
            st.branch = (h != "(detached)").then(|| h.to_string());
        } else if let Some(u) = line.strip_prefix("# branch.upstream ") {
            st.upstream = Some(u.to_string());
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let mut it = ab.split_whitespace();
            st.ahead = it
                .next()
                .and_then(|a| a.trim_start_matches('+').parse().ok())
                .unwrap_or(0);
            st.behind = it
                .next()
                .and_then(|b| b.trim_start_matches('-').parse().ok())
                .unwrap_or(0);
        } else if let Some(path) = line.strip_prefix("? ") {
            st.files.push(FileChange {
                path: super::parse_quoted_path(path),
                status: "??".into(),
            });
        } else {
            let fields = match line.as_bytes().first() {
                Some(b'1') => 9,
                Some(b'2') => 10,
                Some(b'u') => 11,
                _ => continue,
            };
            let parts: Vec<_> = line.splitn(fields, ' ').collect();
            if parts.len() != fields || parts[1].len() != 2 {
                continue;
            }
            let path = if fields == 10 {
                parts[fields - 1].split('\t').next().unwrap_or("")
            } else {
                parts[fields - 1]
            };
            st.files.push(FileChange {
                path: super::parse_quoted_path(path),
                status: parts[1].replace('.', " "),
            });
        }
    }
    st
}
