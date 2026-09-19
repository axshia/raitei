/**
 * PR・コンフリクト状態ストア（担当: WS-H）。
 *
 * - PR 状態は PR サブビュー表示中に定期ポーリング（30 秒）で更新する。ポーリングは `silent` で呼び、
 *   表示中のエラーやローディング表示を乱さない
 * - PR 系とコンフリクト系でローディング・エラーを分けて持つ（片方の失敗が他方の表示を消さないように）
 * - PR 作成・取得で Task.prNumber が変わったら `useProjectStore.getState().upsertTask()` で反映する
 */
import { create } from "zustand";
import { gitApi, prApi, tasksApi, toAppError } from "../api";
import type {
  AgentRunInfo,
  AppError,
  ConflictFileContent,
  ConflictResolution,
  ConflictState,
  CreatePrRequest,
  MergePrRequest,
  PullRequestStatus,
} from "../api";
import { useProjectStore } from "./projectStore";

/** 実行中の操作（ボタンの表示切り替え用） */
export type PrOp = "refresh" | "create" | "merge";
export type ConflictOp = "refresh" | "start" | "resolve" | "agent" | "commit" | "abort";

/** ユーザーに見せる直近の結果メッセージ */
export interface Notice {
  kind: "info" | "success";
  text: string;
}

interface PrState {
  /** undefined = 未取得, null = PR なし */
  prByTask: Record<string, PullRequestStatus | null | undefined>;
  /** PR 状態を最後に取得した時刻（ms） */
  prFetchedAtByTask: Record<string, number | undefined>;
  prOpByTask: Record<string, PrOp | null>;
  prErrorByTask: Record<string, AppError | null>;
  prNoticeByTask: Record<string, Notice | null>;

  conflictByTask: Record<string, ConflictState | undefined>;
  conflictOpByTask: Record<string, ConflictOp | null>;
  /** resolve 中のファイルパス */
  resolvingPathByTask: Record<string, string | null>;
  conflictErrorByTask: Record<string, AppError | null>;
  conflictNoticeByTask: Record<string, Notice | null>;
  /** 競合ファイルの内容（taskId → path → 内容） */
  conflictFileByTask: Record<string, Record<string, ConflictFileContent | undefined>>;
  /** AI に解消を依頼した run（完了を検知して一覧を更新するため） */
  agentRunByTask: Record<string, AgentRunInfo | null>;

  refreshPr(taskId: string, opts?: { silent?: boolean }): Promise<void>;
  createPr(req: CreatePrRequest): Promise<boolean>;
  mergePr(req: MergePrRequest): Promise<boolean>;
  clearPrError(taskId: string): void;

  refreshConflicts(taskId: string, opts?: { silent?: boolean }): Promise<void>;
  startBaseMerge(taskId: string): Promise<void>;
  loadConflictFile(taskId: string, path: string): Promise<void>;
  resolveFile(taskId: string, path: string, resolution: ConflictResolution): Promise<void>;
  askAgentToResolve(taskId: string): Promise<void>;
  /** AI の run が終わったときに呼ぶ（run 情報を消して一覧を再取得） */
  onAgentRunFinished(taskId: string): Promise<void>;
  completeMerge(taskId: string, push: boolean): Promise<void>;
  abortMerge(taskId: string): Promise<void>;
  clearConflictError(taskId: string): void;
}

/** Task.prNumber が PR と食い違っていれば Task を取り直して projectStore に反映する（失敗は無視） */
async function syncTaskPrNumber(taskId: string, pr: PullRequestStatus | null) {
  const task = useProjectStore.getState().findTask(taskId);
  if (!pr || (task && task.prNumber === pr.number)) return;
  try {
    useProjectStore.getState().upsertTask(await tasksApi.getTask(taskId));
  } catch {
    // 一覧表示の更新だけなので握りつぶす
  }
}

export const usePrStore = create<PrState>((set, get) => {
  const patch = <K extends keyof PrState>(key: K, taskId: string, value: unknown) =>
    set((s) => ({ [key]: { ...(s[key] as Record<string, unknown>), [taskId]: value } }) as Partial<PrState>);

  /** PR 系の操作を包む。成功なら true */
  const runPr = async (taskId: string, op: PrOp, fn: () => Promise<void>, silent = false): Promise<boolean> => {
    if (!silent) {
      patch("prOpByTask", taskId, op);
      patch("prErrorByTask", taskId, null);
      patch("prNoticeByTask", taskId, null);
    }
    try {
      await fn();
      return true;
    } catch (e) {
      // ポーリングの失敗は既存表示を残したままエラーだけ出す
      patch("prErrorByTask", taskId, toAppError(e));
      return false;
    } finally {
      if (!silent) patch("prOpByTask", taskId, null);
    }
  };

  const runConflict = async (taskId: string, op: ConflictOp, fn: () => Promise<void>, silent = false) => {
    if (!silent) {
      patch("conflictOpByTask", taskId, op);
      patch("conflictErrorByTask", taskId, null);
      patch("conflictNoticeByTask", taskId, null);
    }
    try {
      await fn();
    } catch (e) {
      patch("conflictErrorByTask", taskId, toAppError(e));
    } finally {
      if (!silent) patch("conflictOpByTask", taskId, null);
    }
  };

  const setPr = (taskId: string, pr: PullRequestStatus | null) => {
    patch("prByTask", taskId, pr);
    patch("prFetchedAtByTask", taskId, Date.now());
  };

  /** 競合状態を保存し、一覧から消えたファイルのキャッシュを捨てる */
  const setConflict = (taskId: string, c: ConflictState) => {
    patch("conflictByTask", taskId, c);
    const cache = get().conflictFileByTask[taskId] ?? {};
    const alive = new Set(c.files.map((f) => f.path));
    const next = Object.fromEntries(Object.entries(cache).filter(([p]) => alive.has(p)));
    patch("conflictFileByTask", taskId, next);
  };

  return {
    prByTask: {},
    prFetchedAtByTask: {},
    prOpByTask: {},
    prErrorByTask: {},
    prNoticeByTask: {},

    conflictByTask: {},
    conflictOpByTask: {},
    resolvingPathByTask: {},
    conflictErrorByTask: {},
    conflictNoticeByTask: {},
    conflictFileByTask: {},
    agentRunByTask: {},

    refreshPr: async (taskId, opts) => {
      // 操作中にポーリングが重なった場合はスキップ
      if (opts?.silent && get().prOpByTask[taskId]) return;
      await runPr(
        taskId,
        "refresh",
        async () => {
          const pr = await prApi.getPullRequest(taskId);
          setPr(taskId, pr);
          await syncTaskPrNumber(taskId, pr);
        },
        opts?.silent,
      );
    },

    createPr: (req) =>
      runPr(req.taskId, "create", async () => {
        const pr = await prApi.createPullRequest(req);
        setPr(req.taskId, pr);
        patch("prNoticeByTask", req.taskId, { kind: "success", text: `PR #${pr.number} を作成しました` });
        await syncTaskPrNumber(req.taskId, pr);
      }),

    mergePr: (req) =>
      runPr(req.taskId, "merge", async () => {
        const pr = await prApi.mergePullRequest(req);
        setPr(req.taskId, pr);
        const text = pr.state === "merged" ? `PR #${pr.number} をマージしました` : `マージを要求しました（状態: ${pr.state}）`;
        patch("prNoticeByTask", req.taskId, { kind: "success", text });
      }),

    clearPrError: (taskId) => patch("prErrorByTask", taskId, null),

    refreshConflicts: (taskId, opts) =>
      runConflict(taskId, "refresh", async () => setConflict(taskId, await gitApi.getConflictState(taskId)), opts?.silent),

    startBaseMerge: (taskId) =>
      runConflict(taskId, "start", async () => {
        const c = await gitApi.startBaseMerge(taskId);
        setConflict(taskId, c);
        const base = c.baseRef ?? "base";
        let text: string;
        if (c.files.length > 0) text = `${base} の取り込みで ${c.files.length} 件のコンフリクトが発生しました`;
        else if (c.mergeInProgress) text = `${base} をコンフリクトなしで取り込みました。コミットして完了してください`;
        else text = `${base} は取り込み済みです（変更なし）`;
        patch("conflictNoticeByTask", taskId, { kind: c.files.length > 0 ? "info" : "success", text });
      }),

    loadConflictFile: async (taskId, path) => {
      try {
        const content = await gitApi.readConflictFile(taskId, path);
        set((s) => ({
          conflictFileByTask: {
            ...s.conflictFileByTask,
            [taskId]: { ...(s.conflictFileByTask[taskId] ?? {}), [path]: content },
          },
        }));
      } catch (e) {
        patch("conflictErrorByTask", taskId, toAppError(e));
      }
    },

    resolveFile: async (taskId, path, resolution) => {
      patch("resolvingPathByTask", taskId, path);
      await runConflict(taskId, "resolve", async () =>
        setConflict(taskId, await gitApi.resolveConflictFile(taskId, path, resolution)),
      );
      patch("resolvingPathByTask", taskId, null);
    },

    askAgentToResolve: (taskId) =>
      runConflict(taskId, "agent", async () => {
        const info = await gitApi.requestAgentConflictResolution(taskId);
        patch("agentRunByTask", taskId, info);
        patch("conflictNoticeByTask", taskId, {
          kind: "info",
          text: `${info.agent} に解消を依頼しました。進捗はチャットで確認できます`,
        });
      }),

    onAgentRunFinished: async (taskId) => {
      if (!get().agentRunByTask[taskId]) return;
      patch("agentRunByTask", taskId, null);
      await get().refreshConflicts(taskId, { silent: true });
      const c = get().conflictByTask[taskId];
      const text =
        c && c.files.length > 0
          ? `AI の作業が終わりました。未解決が ${c.files.length} 件残っています`
          : "AI の作業が終わりました。内容を確認してコミットしてください";
      patch("conflictNoticeByTask", taskId, { kind: "info", text });
    },

    completeMerge: (taskId, push) =>
      runConflict(taskId, "commit", async () => {
        setConflict(taskId, await gitApi.completeBaseMerge(taskId, push));
        patch("conflictNoticeByTask", taskId, {
          kind: "success",
          text: push ? "マージコミットを作成して push しました" : "マージコミットを作成しました（未 push）",
        });
        // コンフリクト状態が変わるので PR も取り直す
        if (push) void get().refreshPr(taskId, { silent: true });
      }),

    abortMerge: (taskId) =>
      runConflict(taskId, "abort", async () => {
        await gitApi.abortBaseMerge(taskId);
        patch("agentRunByTask", taskId, null);
        setConflict(taskId, await gitApi.getConflictState(taskId));
        patch("conflictNoticeByTask", taskId, { kind: "info", text: "base の取り込みを中止しました" });
      }),

    clearConflictError: (taskId) => patch("conflictErrorByTask", taskId, null),
  };
});
