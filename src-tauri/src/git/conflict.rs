//! base 取り込みとコンフリクト解消（担当: WS-C）。
//!
//! フロー（タスクの worktree 上で実行）:
//! 1. `merge_base_into`: `git fetch origin <base>` → `git merge --no-ff --no-commit origin/<base>`
//!    （origin が無ければローカル `<base>`）。競合があれば ConflictState.files に列挙
//! 2. ファイルごとに `resolve_file`（ours / theirs / markResolved）
//!    または AI に依頼（コマンド層がエージェントへプロンプト送信）→ 完了後 markResolved
//! 3. `commit_merge`: 未解決が無いことを確認して `git commit --no-edit`
//! 4. push は `repo::push`
//!
//! 中止は `abort_merge`（`git merge --abort`）。

use std::path::{Component, Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::shell_env::{run, ShellEnv};

use super::types::{
    ConflictFile, ConflictFileContent, ConflictKind, ConflictResolution, ConflictState,
};
use super::{git, repo, status};

/// base ブランチを取り込む。未コミット変更や既存のマージは上書きしない。
pub fn merge_base_into(
    env: &ShellEnv,
    worktree: &Path,
    base_branch: &str,
) -> AppResult<ConflictState> {
    repo::validate_branch(base_branch)?;
    let before = status::status(env, worktree)?;
    if before.merge_in_progress || !before.files.is_empty() {
        return Err(AppError::InvalidInput(
            "マージ前に未コミット変更と進行中のマージを解消してください".into(),
        ));
    }
    let base_ref = if repo::has_origin(env, worktree)? {
        repo::fetch(env, worktree, base_branch)?;
        format!("origin/{base_branch}")
    } else {
        base_branch.to_string()
    };
    let out = run(
        env,
        "git",
        &["merge", "--no-ff", "--no-commit", "--", &base_ref],
        worktree,
    )?;
    let mut state = conflict_state(env, worktree)?;
    // Exit 1 is expected for conflicts, but unrelated merge errors must propagate.
    if !out.success() && !(state.merge_in_progress && !state.files.is_empty()) {
        return Err(AppError::Git(format!(
            "base のマージに失敗: {}\n{}",
            out.stderr.trim(),
            out.stdout.trim()
        )));
    }
    if state.merge_in_progress {
        let head = git(env, worktree, &["rev-parse", "--verify", "MERGE_HEAD"])?;
        // Stored in this worktree's git directory, never in tracked files. Tie it to
        // MERGE_HEAD so an externally aborted/restarted merge cannot reuse stale data.
        std::fs::write(
            metadata_path(env, worktree)?,
            serde_json::to_vec(&(head.trim(), &base_ref))?,
        )?;
        state.base_ref = Some(base_ref);
    }
    Ok(state)
}

pub fn conflict_state(env: &ShellEnv, worktree: &Path) -> AppResult<ConflictState> {
    let st = status::status(env, worktree)?;
    if !st.merge_in_progress {
        return Ok(ConflictState::default());
    }
    let out = git(
        env,
        worktree,
        &["status", "--porcelain=v1", "--untracked-files=no"],
    )?;
    let files = parse_unmerged(&out);
    let head = git(env, worktree, &["rev-parse", "--verify", "MERGE_HEAD"])?;
    let base_ref = match std::fs::read(metadata_path(env, worktree)?) {
        Ok(bytes) => serde_json::from_slice::<(String, String)>(&bytes)
            .ok()
            .filter(|(saved_head, _)| saved_head == head.trim())
            .map(|(_, base)| base),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    Ok(ConflictState {
        merge_in_progress: true,
        base_ref,
        ready_to_commit: files.is_empty(),
        files,
    })
}

fn metadata_path(env: &ShellEnv, worktree: &Path) -> AppResult<PathBuf> {
    let path = git(
        env,
        worktree,
        &["rev-parse", "--git-path", "RAITEI_MERGE_BASE"],
    )?;
    Ok(worktree.join(path.trim_end_matches('\n')))
}

fn remove_metadata(env: &ShellEnv, worktree: &Path) -> AppResult<()> {
    match std::fs::remove_file(metadata_path(env, worktree)?) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Only an exact unmerged, repository-relative path may be operated on.
fn conflict_path(env: &ShellEnv, worktree: &Path, path: &str) -> AppResult<PathBuf> {
    let relative = Path::new(path);
    if path.is_empty()
        || path.contains('\0')
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(AppError::InvalidInput(
            "競合ファイルはリポジトリ内の相対パスで指定してください".into(),
        ));
    }
    let state = conflict_state(env, worktree)?;
    if !state.files.iter().any(|f| f.path == path) {
        return Err(AppError::InvalidInput(format!(
            "未解決の競合ファイルではありません: {path}"
        )));
    }
    let root = worktree.canonicalize()?;
    let full = root.join(relative);
    // Do not traverse a directory symlink outside the worktree. Missing parents
    // are allowed for a manually deleted conflict file.
    let mut parent = full.parent();
    while let Some(p) = parent {
        match p.canonicalize() {
            Ok(actual) => {
                if !actual.starts_with(&root) {
                    return Err(AppError::InvalidInput(
                        "競合ファイルの親が worktree の外を参照しています".into(),
                    ));
                }
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => parent = p.parent(),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(full)
}

fn stage_exists(env: &ShellEnv, worktree: &Path, path: &str, stage: u8) -> AppResult<bool> {
    let out = git(
        env,
        worktree,
        &["--literal-pathspecs", "ls-files", "--unmerged", "--", path],
    )?;
    Ok(out.lines().any(|line| {
        line.split_once('\t')
            .and_then(|(entry, _)| entry.split_whitespace().nth(2))
            == Some(if stage == 2 { "2" } else { "3" })
    }))
}

fn stage_content(
    env: &ShellEnv,
    worktree: &Path,
    path: &str,
    stage: u8,
) -> AppResult<Option<String>> {
    if !stage_exists(env, worktree, path, stage)? {
        return Ok(None);
    }
    let spec = format!(":{stage}:{path}");
    let out = env
        .command("git")
        .args(["show", &spec])
        .current_dir(worktree)
        .output()
        .map_err(|e| AppError::Command(format!("git の起動に失敗: {e}")))?;
    if !out.status.success() {
        return Err(AppError::Git(
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ));
    }
    Ok(parse_text_content(out.stdout))
}

fn parse_text_content(bytes: Vec<u8>) -> Option<String> {
    if bytes.contains(&0) {
        None
    } else {
        String::from_utf8(bytes).ok()
    }
}

/// 削除済み・バイナリ・symlink の作業ツリー内容は None。
pub fn read_conflict_file(
    env: &ShellEnv,
    worktree: &Path,
    path: &str,
) -> AppResult<ConflictFileContent> {
    let full = conflict_path(env, worktree, path)?;
    let working = match std::fs::symlink_metadata(&full) {
        Ok(meta) if meta.is_file() => parse_text_content(std::fs::read(&full)?),
        Ok(_) => None,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    Ok(ConflictFileContent {
        path: path.to_string(),
        working,
        ours: stage_content(env, worktree, path, 2)?,
        theirs: stage_content(env, worktree, path, 3)?,
    })
}

pub fn resolve_file(
    env: &ShellEnv,
    worktree: &Path,
    path: &str,
    resolution: ConflictResolution,
) -> AppResult<ConflictState> {
    conflict_path(env, worktree, path)?;
    let stage = match resolution {
        ConflictResolution::Ours => Some(2),
        ConflictResolution::Theirs => Some(3),
        ConflictResolution::MarkResolved => None,
    };
    if let Some(stage) = stage {
        if stage_exists(env, worktree, path, stage)? {
            git(
                env,
                worktree,
                &[
                    "--literal-pathspecs",
                    "checkout",
                    if stage == 2 { "--ours" } else { "--theirs" },
                    "--",
                    path,
                ],
            )?;
        } else {
            // Choosing the deleted side of modify/delete or delete/delete conflicts.
            git(
                env,
                worktree,
                &["--literal-pathspecs", "rm", "-f", "--", path],
            )?;
            return conflict_state(env, worktree);
        }
    }
    git(
        env,
        worktree,
        &["--literal-pathspecs", "add", "-A", "--", path],
    )?;
    conflict_state(env, worktree)
}

pub fn abort_merge(env: &ShellEnv, worktree: &Path) -> AppResult<()> {
    if conflict_state(env, worktree)?.merge_in_progress {
        git(env, worktree, &["merge", "--abort"])?;
    }
    remove_metadata(env, worktree)
}

pub fn commit_merge(env: &ShellEnv, worktree: &Path) -> AppResult<()> {
    let state = conflict_state(env, worktree)?;
    if !state.files.is_empty() {
        return Err(AppError::InvalidInput(
            "未解決のコンフリクトが残っています".into(),
        ));
    }
    // Already up-to-date and retries after a failed push have no merge to commit.
    if state.merge_in_progress {
        git(env, worktree, &["commit", "--no-edit"])?;
    }
    remove_metadata(env, worktree)
}

/// `git status --porcelain=v1` から未解決ファイルを抽出（純粋関数）。
pub fn parse_unmerged(porcelain_v1: &str) -> Vec<ConflictFile> {
    porcelain_v1
        .lines()
        .filter(|l| l.len() > 3)
        .filter_map(|l| {
            let kind = match l.get(..2)? {
                "UU" => ConflictKind::BothModified,
                "AA" => ConflictKind::BothAdded,
                "DU" => ConflictKind::DeletedByUs,
                "UD" => ConflictKind::DeletedByThem,
                "AU" | "UA" | "DD" => ConflictKind::Other,
                _ => return None,
            };
            Some(ConflictFile {
                path: super::parse_quoted_path(l.get(3..)?),
                kind,
            })
        })
        .collect()
}

/// AI エージェントへの解消依頼プロンプトを組み立てる（純粋関数）。
pub fn build_agent_prompt(base_ref: &str, files: &[ConflictFile]) -> String {
    let list = files
        .iter()
        .map(|f| format!("- {}", f.path))
        .collect::<Vec<_>>()
        .join("\n");
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
