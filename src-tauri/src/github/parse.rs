//! `gh pr view --json` のパース（担当: WS-D）。純粋関数のみ。

use crate::error::{AppError, AppResult};

use super::types::{ChecksSummary, PullRequestStatus};

/// `gh pr view --json <PR_JSON_FIELDS>` の出力を変換する。
pub fn parse_pr_view(_json: &str) -> AppResult<PullRequestStatus> {
    Err(AppError::NotImplemented("github::parse::parse_pr_view"))
}

/// statusCheckRollup 配列を集計する。
/// CheckRun: `status`(QUEUED/IN_PROGRESS/COMPLETED) + `conclusion`(SUCCESS/FAILURE/...)
/// StatusContext: `state`(PENDING/SUCCESS/FAILURE/ERROR)
pub fn summarize_checks(_rollup: &serde_json::Value) -> ChecksSummary {
    ChecksSummary::default()
}
