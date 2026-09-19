/**
 * PR・コンフリクト状態ストア（担当: WS-H）。
 * PR 状態はタブ表示中に定期ポーリング（例: 30 秒）で更新する想定。
 */
import { create } from "zustand";
import { gitApi, prApi, toAppError } from "../api";
import type { AppError, ConflictResolution, ConflictState, CreatePrRequest, MergePrRequest, PullRequestStatus } from "../api";

interface PrState {
  prByTask: Record<string, PullRequestStatus | null | undefined>;
  conflictByTask: Record<string, ConflictState | undefined>;
  loadingByTask: Record<string, boolean>;
  errorByTask: Record<string, AppError | null>;

  refreshPr(taskId: string): Promise<void>;
  createPr(req: CreatePrRequest): Promise<void>;
  mergePr(req: MergePrRequest): Promise<void>;

  refreshConflicts(taskId: string): Promise<void>;
  startBaseMerge(taskId: string): Promise<void>;
  resolveFile(taskId: string, path: string, resolution: ConflictResolution): Promise<void>;
  askAgentToResolve(taskId: string): Promise<void>;
  completeMerge(taskId: string, push: boolean): Promise<void>;
  abortMerge(taskId: string): Promise<void>;
}

export const usePrStore = create<PrState>((set) => {
  const run = async (taskId: string, fn: () => Promise<void>) => {
    set((s) => ({
      loadingByTask: { ...s.loadingByTask, [taskId]: true },
      errorByTask: { ...s.errorByTask, [taskId]: null },
    }));
    try {
      await fn();
    } catch (e) {
      set((s) => ({ errorByTask: { ...s.errorByTask, [taskId]: toAppError(e) } }));
    } finally {
      set((s) => ({ loadingByTask: { ...s.loadingByTask, [taskId]: false } }));
    }
  };
  const setPr = (taskId: string, pr: PullRequestStatus | null) =>
    set((s) => ({ prByTask: { ...s.prByTask, [taskId]: pr } }));
  const setConflict = (taskId: string, c: ConflictState) =>
    set((s) => ({ conflictByTask: { ...s.conflictByTask, [taskId]: c } }));

  return {
    prByTask: {},
    conflictByTask: {},
    loadingByTask: {},
    errorByTask: {},

    refreshPr: (taskId) => run(taskId, async () => setPr(taskId, await prApi.getPullRequest(taskId))),
    createPr: (req) => run(req.taskId, async () => setPr(req.taskId, await prApi.createPullRequest(req))),
    mergePr: (req) => run(req.taskId, async () => setPr(req.taskId, await prApi.mergePullRequest(req))),

    refreshConflicts: (taskId) => run(taskId, async () => setConflict(taskId, await gitApi.getConflictState(taskId))),
    startBaseMerge: (taskId) => run(taskId, async () => setConflict(taskId, await gitApi.startBaseMerge(taskId))),
    resolveFile: (taskId, path, resolution) =>
      run(taskId, async () => setConflict(taskId, await gitApi.resolveConflictFile(taskId, path, resolution))),
    askAgentToResolve: (taskId) =>
      run(taskId, async () => {
        await gitApi.requestAgentConflictResolution(taskId);
      }),
    completeMerge: (taskId, push) =>
      run(taskId, async () => setConflict(taskId, await gitApi.completeBaseMerge(taskId, push))),
    abortMerge: (taskId) =>
      run(taskId, async () => {
        await gitApi.abortBaseMerge(taskId);
        setConflict(taskId, await gitApi.getConflictState(taskId));
      }),
  };
});
