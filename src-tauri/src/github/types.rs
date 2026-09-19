//! PR 関連の IPC 型（契約: 凍結）。TS 側は `src/api/types.ts`。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

/// gh の `mergeable`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Mergeable {
    Mergeable,
    Conflicting,
    Unknown,
}

/// CI チェック 1 件（statusCheckRollup の CheckRun / StatusContext を統合）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckRun {
    pub name: String,
    /// pending / success / failure / neutral / skipped / cancelled / unknown に正規化
    pub status: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChecksSummary {
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
    pub checks: Vec<CheckRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub author: String,
    /// APPROVED / CHANGES_REQUESTED / COMMENTED / DISMISSED / PENDING
    pub state: String,
    pub submitted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestStatus {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub state: PrState,
    pub is_draft: bool,
    pub head_branch: String,
    pub base_branch: String,
    pub mergeable: Mergeable,
    /// gh の mergeStateStatus（CLEAN / DIRTY / BLOCKED / BEHIND / UNSTABLE / HAS_HOOKS / UNKNOWN / DRAFT）
    pub merge_state_status: String,
    /// APPROVED / CHANGES_REQUESTED / REVIEW_REQUIRED / None
    pub review_decision: Option<String>,
    pub reviews: Vec<Review>,
    pub checks: ChecksSummary,
    /// mergeable == Conflicting または merge_state_status == DIRTY
    pub has_conflicts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrRequest {
    pub task_id: String,
    pub title: String,
    pub body: String,
    /// 省略時は Task.base_branch
    pub base: Option<String>,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePrRequest {
    pub task_id: String,
    pub method: MergeMethod,
    /// マージ後にリモートブランチを削除する（ローカル worktree には触れない）
    #[serde(default)]
    pub delete_remote_branch: bool,
}
