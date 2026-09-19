//! git CLI ラッパー（担当: WS-C）。
//!
//! すべて `git` CLI を [`crate::shell_env`] 経由で呼ぶ。関数は状態を持たない自由関数で、
//! 第 1 引数に `&ShellEnv`、第 2 引数に操作対象ディレクトリ（メインリポジトリ or worktree）を取る。
//! 出力パースは副作用のない `parse_*` 関数に分離し、ユニットテストする。
//!
//! - `repo`     : リポジトリ判定・初期化・ブランチ情報・fetch/push
//! - `worktree` : worktree 追加/一覧/削除
//! - `status`   : 作業ツリー状態
//! - `conflict` : base 取り込みとコンフリクト解消

pub mod conflict;
pub mod repo;
pub mod status;
pub mod types;
pub mod worktree;

pub use types::*;

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::shell_env::{run, ShellEnv};

/// git を実行し、成功時は stdout を返す。失敗時は stderr を含む `AppError::Git`。
pub fn git(env: &ShellEnv, cwd: &Path, args: &[&str]) -> AppResult<String> {
    let out = run(env, "git", args, cwd)?;
    if out.success() {
        Ok(out.stdout)
    } else {
        Err(AppError::Git(format!(
            "git {} 失敗 (exit {}): {}",
            args.join(" "),
            out.status,
            out.stderr.trim()
        )))
    }
}
