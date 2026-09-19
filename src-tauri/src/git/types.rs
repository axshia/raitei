//! git 関連の IPC 型（契約: 凍結）。TS 側は `src/api/types.ts`。

use serde::{Deserialize, Serialize};

/// `git worktree list --porcelain` の 1 エントリ。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub path: String,
    /// HEAD の commit SHA
    pub head: Option<String>,
    /// `refs/heads/` を除いたブランチ名。detached なら None
    pub branch: Option<String>,
    pub is_main: bool,
    pub is_bare: bool,
    pub is_detached: bool,
    pub locked: bool,
    pub prunable: bool,
    /// raitei のタスクに紐づく場合そのタスク ID（コマンド層で埋める）
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// porcelain の XY ステータス（例: " M", "A ", "UU", "??"）
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<FileChange>,
    pub merge_in_progress: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    /// UU
    BothModified,
    /// AA
    BothAdded,
    /// DU
    DeletedByUs,
    /// UD
    DeletedByThem,
    /// その他（AU, UA, DD）
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path: String,
    pub kind: ConflictKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConflictState {
    /// マージ中（MERGE_HEAD がある）か
    pub merge_in_progress: bool,
    /// 取り込み中の ref（例: origin/main）
    pub base_ref: Option<String>,
    /// 未解決の競合ファイル
    pub files: Vec<ConflictFile>,
    /// 解決済み（add 済み）でコミット待ちのファイルがあるか
    pub ready_to_commit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFileContent {
    pub path: String,
    /// 作業ツリー上の内容（競合マーカー付き）。バイナリ・削除済みなら None
    pub working: Option<String>,
    /// stage 2（ours = タスクブランチ側）
    pub ours: Option<String>,
    /// stage 3（theirs = 取り込んだ base 側）
    pub theirs: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ConflictResolution {
    /// `git checkout --ours` + add
    Ours,
    /// `git checkout --theirs` + add
    Theirs,
    /// 手動/AI で編集済みとして add のみ
    MarkResolved,
}
