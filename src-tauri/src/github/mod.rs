//! gh CLI ラッパー（担当: WS-D）。
//!
//! - `gh`    : gh コマンド実行（認証確認・PR 取得/作成/マージ）
//! - `parse` : `gh pr view --json` のパース（純粋関数・ユニットテスト対象）
//!
//! 取得フィールド（[`PR_JSON_FIELDS`]）を `gh pr view <branch|number> --json ...` で取得し
//! [`parse::parse_pr_view`] で [`PullRequestStatus`] に変換する。

pub mod gh;
pub mod parse;
pub mod types;

pub use types::*;

pub const PR_JSON_FIELDS: &str = "number,url,title,state,isDraft,headRefName,baseRefName,mergeable,mergeStateStatus,reviewDecision,reviews,statusCheckRollup";
