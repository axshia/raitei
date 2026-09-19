//! base 取り込みとコンフリクト解消（担当: WS-C）。
//!
//! フロー（タスクの worktree 上で実行）:
//! 1. `merge_base_into`: `git fetch origin <base>` → `git merge --no-ff --no-commit origin/<base>`
//!    （origin が無ければローカル `<base>`）。競合があれば ConflictState.files に列挙
//! 2. ファイルごとに `resolve_file`（ours / theirs / markResolved）
//!    または AI に依頼（コマンド層がエージェントへプロンプト送信）→ 完了後 markResolved
//! 3. `commit_merge`: 未解決が無いことを確認して `git commit --no-edit`
//! 4. push は `repo::push`
//! 中止は `abort_merge`（`git merge --abort`）。

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::shell_env::ShellEnv;

use super::types::{ConflictFile, ConflictFileContent, ConflictKind, ConflictResolution, ConflictState};

/// base ブランチを取り込む。競合が無ければマージ完了状態（ready_to_commit=true）を返す。
pub fn merge_base_into(_env: &ShellEnv, _worktree: &Path, _base_branch: &str) -> AppResult<ConflictState> {
    Err(AppError::NotImplemented("git::conflict::merge_base_into"))
}

/// 現在のマージ/競合状態。マージ中でなければ `ConflictState::default()`。
pub fn conflict_state(_env: &ShellEnv, _worktree: &Path) -> AppResult<ConflictState> {
    Ok(ConflictState::default())
}

/// 競合ファイルの作業ツリー版と ours/theirs（`git show :2:<path>` / `:3:<path>`）。
pub fn read_conflict_file(_env: &ShellEnv, _worktree: &Path, _path: &str) -> AppResult<ConflictFileContent> {
    Err(AppError::NotImplemented("git::conflict::read_conflict_file"))
}

/// 1 ファイルを解決して add する。解決後の状態を返す。
pub fn resolve_file(
    _env: &ShellEnv,
    _worktree: &Path,
    _path: &str,
    _resolution: ConflictResolution,
) -> AppResult<ConflictState> {
    Err(AppError::NotImplemented("git::conflict::resolve_file"))
}

/// `git merge --abort`。
pub fn abort_merge(_env: &ShellEnv, _worktree: &Path) -> AppResult<()> {
    Err(AppError::NotImplemented("git::conflict::abort_merge"))
}

/// 未解決ファイルが無ければ `git commit --no-edit`。あれば `AppError::InvalidInput`。
pub fn commit_merge(_env: &ShellEnv, _worktree: &Path) -> AppResult<()> {
    Err(AppError::NotImplemented("git::conflict::commit_merge"))
}

/// `git status --porcelain=v1` から未解決ファイルを抽出（純粋関数）。
pub fn parse_unmerged(porcelain_v1: &str) -> Vec<ConflictFile> {
    porcelain_v1
        .lines()
        .filter(|l| l.len() > 3)
        .filter_map(|l| {
            let kind = match &l[..2] {
                "UU" => ConflictKind::BothModified,
                "AA" => ConflictKind::BothAdded,
                "DU" => ConflictKind::DeletedByUs,
                "UD" => ConflictKind::DeletedByThem,
                "AU" | "UA" | "DD" => ConflictKind::Other,
                _ => return None,
            };
            Some(ConflictFile {
                path: l[3..].to_string(),
                kind,
            })
        })
        .collect()
}

/// AI エージェントへの解消依頼プロンプトを組み立てる（純粋関数）。
pub fn build_agent_prompt(base_ref: &str, files: &[ConflictFile]) -> String {
    let list = files.iter().map(|f| format!("- {}", f.path)).collect::<Vec<_>>().join("\n");
    format!(
        "このブランチに {base_ref} をマージしたところ、以下のファイルでコンフリクトが発生しました。\n\
         {list}\n\n\
         各ファイルの競合マーカー（<<<<<<< / ======= / >>>>>>>）を解消し、両方の変更意図を保った正しいコードにしてください。\n\
         解消したら `git add <file>` してください。コミットはしないでください。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmerged() {
        let v = parse_unmerged("UU src/a.rs\n M b.rs\nDU c.rs\n");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].kind, ConflictKind::BothModified);
        assert_eq!(v[1].path, "c.rs");
    }
}
