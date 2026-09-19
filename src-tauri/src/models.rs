//! フロントと共有するドメイン型（契約: 凍結）。
//!
//! すべて camelCase で JSON 化される。TS 側は `src/api/types.ts` に同名の型がある。
//! フィールドを追加する場合は Rust / TS の両方を同じコミットで更新すること。

use serde::{Deserialize, Serialize};

/// 日時は RFC3339 文字列でやり取りする。
pub type Timestamp = String;

pub fn now() -> Timestamp {
    chrono::Utc::now().to_rfc3339()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    Claude,
    Codex,
}

impl AgentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            _ => None,
        }
    }
}

/// エージェントに与える権限レベル。
/// - `safe`: claude `--permission-mode acceptEdits` / codex `-s workspace-write`
/// - `full`: claude `--permission-mode bypassPermissions` / codex `--dangerously-bypass-approvals-and-sandbox`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PermissionLevel {
    #[default]
    Safe,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// メインリポジトリの絶対パス
    pub repo_path: String,
    /// 既定の base ブランチ（例: main）
    pub default_branch: String,
    pub created_at: Timestamp,
}

/// タスク = worktree + ブランチ + エージェントセッション。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub branch: String,
    pub base_branch: String,
    /// worktree の絶対パス
    pub worktree_path: String,
    pub agent: AgentKind,
    pub permission: PermissionLevel,
    /// claude の session_id / codex の thread_id。初回実行後に保存し、以降 resume に使う。
    pub agent_session_id: Option<String>,
    /// 作成済み PR 番号（あれば）
    pub pr_number: Option<u64>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    /// 親ディレクトリ
    pub parent_dir: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub project_id: String,
    pub title: String,
    /// 作成するブランチ名
    pub branch: String,
    /// 省略時は Project.default_branch
    pub base_branch: Option<String>,
    pub agent: AgentKind,
    #[serde(default)]
    pub permission: PermissionLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    pub task_id: String,
    pub title: Option<String>,
    pub agent: Option<AgentKind>,
    pub permission: Option<PermissionLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTaskOptions {
    /// worktree ディレクトリを削除する
    pub remove_worktree: bool,
    /// ローカルブランチも削除する
    pub delete_branch: bool,
    /// 未コミット変更があっても強制する
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentInfo {
    /// 解決済み PATH
    pub path: String,
    pub git: ToolInfo,
    pub gh: ToolInfo,
    pub claude: ToolInfo,
    pub codex: ToolInfo,
    pub gh_authenticated: bool,
}
