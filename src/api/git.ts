import { call } from "./invoke";
import type {
  AgentRunInfo,
  ConflictFileContent,
  ConflictResolution,
  ConflictState,
  GitStatus,
  WorktreeInfo,
} from "./types";

export const listWorktrees = (projectId: string) => call<WorktreeInfo[]>("list_worktrees", { projectId });
export const getGitStatus = (taskId: string) => call<GitStatus>("get_git_status", { taskId });

// ---- コンフリクト解消フロー ----

/** base を取り込む（fetch + merge）。競合があれば files に列挙される */
export const startBaseMerge = (taskId: string) => call<ConflictState>("start_base_merge", { taskId });
export const getConflictState = (taskId: string) => call<ConflictState>("get_conflict_state", { taskId });
export const readConflictFile = (taskId: string, path: string) =>
  call<ConflictFileContent>("read_conflict_file", { taskId, path });
export const resolveConflictFile = (taskId: string, path: string, resolution: ConflictResolution) =>
  call<ConflictState>("resolve_conflict_file", { taskId, path, resolution });
export const abortBaseMerge = (taskId: string) => call<void>("abort_base_merge", { taskId });
/** マージコミット作成（push=true なら push も） */
export const completeBaseMerge = (taskId: string, push: boolean) =>
  call<ConflictState>("complete_base_merge", { taskId, push });
/** タスクのエージェントに競合解消を依頼（結果は agent://event で届く） */
export const requestAgentConflictResolution = (taskId: string) =>
  call<AgentRunInfo>("request_agent_conflict_resolution", { taskId });
