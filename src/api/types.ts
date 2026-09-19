/**
 * フロント ↔ Rust の契約型（凍結）。
 *
 * Rust 側の対応:
 * - models.rs / agent/types.rs / git/types.rs / github/types.rs / error.rs
 * フィールドを変更する場合は Rust と同じコミットで更新すること。
 */

// ---------- 共通 ----------

export type Timestamp = string; // RFC3339

export type AppErrorKind =
  | "notImplemented"
  | "notFound"
  | "invalidInput"
  | "git"
  | "gh"
  | "agent"
  | "command"
  | "db"
  | "io";

/** Rust の AppError（invoke が reject したときの値） */
export interface AppError {
  kind: AppErrorKind;
  message: string;
}

export type AgentKind = "claude" | "codex";
export type PermissionLevel = "safe" | "full";

// ---------- models.rs ----------

export interface Project {
  id: string;
  name: string;
  repoPath: string;
  defaultBranch: string;
  createdAt: Timestamp;
}

export interface Task {
  id: string;
  projectId: string;
  title: string;
  branch: string;
  baseBranch: string;
  worktreePath: string;
  agent: AgentKind;
  permission: PermissionLevel;
  agentSessionId: string | null;
  prNumber: number | null;
  createdAt: Timestamp;
  updatedAt: Timestamp;
}

export interface CreateProjectRequest {
  parentDir: string;
  name: string;
}

export interface CreateTaskRequest {
  projectId: string;
  title: string;
  branch: string;
  baseBranch?: string | null;
  agent: AgentKind;
  permission?: PermissionLevel;
}

export interface UpdateTaskRequest {
  taskId: string;
  title?: string | null;
  agent?: AgentKind | null;
  permission?: PermissionLevel | null;
}

export interface DeleteTaskOptions {
  removeWorktree: boolean;
  deleteBranch: boolean;
  force: boolean;
}

export interface ToolInfo {
  name: string;
  path: string | null;
  version: string | null;
}

export interface EnvironmentInfo {
  path: string;
  git: ToolInfo;
  gh: ToolInfo;
  claude: ToolInfo;
  codex: ToolInfo;
  ghAuthenticated: boolean;
}

// ---------- git/types.rs ----------

export interface WorktreeInfo {
  path: string;
  head: string | null;
  branch: string | null;
  isMain: boolean;
  isBare: boolean;
  isDetached: boolean;
  locked: boolean;
  prunable: boolean;
  taskId: string | null;
}

export interface FileChange {
  path: string;
  /** porcelain XY（例: " M", "UU", "??"） */
  status: string;
}

export interface GitStatus {
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  files: FileChange[];
  mergeInProgress: boolean;
}

export type ConflictKind = "bothModified" | "bothAdded" | "deletedByUs" | "deletedByThem" | "other";

export interface ConflictFile {
  path: string;
  kind: ConflictKind;
}

export interface ConflictState {
  mergeInProgress: boolean;
  baseRef: string | null;
  files: ConflictFile[];
  readyToCommit: boolean;
}

export interface ConflictFileContent {
  path: string;
  working: string | null;
  ours: string | null;
  theirs: string | null;
}

export type ConflictResolution = "ours" | "theirs" | "markResolved";

// ---------- github/types.rs ----------

export type PrState = "open" | "closed" | "merged";
export type Mergeable = "mergeable" | "conflicting" | "unknown";

export interface CheckRun {
  name: string;
  /** pending / success / failure / neutral / skipped / cancelled / unknown */
  status: string;
  url: string | null;
}

export interface ChecksSummary {
  total: number;
  passed: number;
  failed: number;
  pending: number;
  checks: CheckRun[];
}

export interface Review {
  author: string;
  state: string;
  submittedAt: string | null;
}

export interface PullRequestStatus {
  number: number;
  url: string;
  title: string;
  state: PrState;
  isDraft: boolean;
  headBranch: string;
  baseBranch: string;
  mergeable: Mergeable;
  mergeStateStatus: string;
  reviewDecision: string | null;
  reviews: Review[];
  checks: ChecksSummary;
  hasConflicts: boolean;
}

export interface CreatePrRequest {
  taskId: string;
  title: string;
  body: string;
  base?: string | null;
  draft?: boolean;
}

export type MergeMethod = "merge" | "squash" | "rebase";

export interface MergePrRequest {
  taskId: string;
  method: MergeMethod;
  deleteRemoteBranch?: boolean;
}

// ---------- agent/types.rs ----------

/** Tauri イベント名 */
export const AGENT_EVENT = "agent://event";

/** 正規化イベント（`type` でタグ付け。フィールドは Rust の snake_case のまま） */
export type AgentEvent =
  | { type: "user_message"; text: string }
  | { type: "session_started"; session_id: string; model: string | null }
  | { type: "assistant_text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "tool_use"; id: string; name: string; input: unknown }
  | { type: "tool_result"; tool_use_id: string; output: string; is_error: boolean }
  | {
      type: "result";
      is_error: boolean;
      text: string | null;
      duration_ms: number | null;
      cost_usd: number | null;
      usage: unknown;
    }
  | { type: "error"; message: string }
  | { type: "stderr"; line: string }
  | { type: "run_finished"; exit_code: number | null; cancelled: boolean };

export type AgentEventType = AgentEvent["type"];

export interface AgentEventEnvelope {
  taskId: string;
  runId: string;
  agent: AgentKind;
  /** タスク内で単調増加。重複排除キー */
  seq: number;
  timestamp: Timestamp;
  event: AgentEvent;
}

export interface AgentRunInfo {
  runId: string;
  taskId: string;
  agent: AgentKind;
}

export interface SendMessageRequest {
  taskId: string;
  text: string;
}

export interface AgentRunState {
  taskId: string;
  running: boolean;
  runId: string | null;
}
