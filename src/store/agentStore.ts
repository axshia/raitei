/**
 * エージェント会話ストア（担当: WS-G）。
 *
 * - `initAgentEventBridge()` をアプリ起動時に 1 回呼び、`agent://event` を購読して applyEvent する
 * - タブを開いたら `loadHistory(taskId)` で過去分を取得する。ライブイベントとの重複は seq で除く
 * - 表示用の整形（tool_use と tool_result の対応付け等）は `features/chat/chatModel.ts` の
 *   `selectChatItems` が行う。ストアは seq 昇順・重複なしのイベント列だけを持つ
 */
import { create } from "zustand";
import { agentApi, toAppError } from "../api";
import type { AgentEventEnvelope, AppError } from "../api";

/** 送信直後、バックエンドから user_message が届くまでの仮表示 */
export interface PendingMessage {
  text: string;
  sentAt: number;
}

interface AgentState {
  /** タスクごとのイベント列（seq 昇順・重複なし） */
  eventsByTask: Record<string, AgentEventEnvelope[]>;
  /** 実行中フラグ（タブの ● 表示にも使う） */
  runningByTask: Record<string, boolean>;
  /** 実行中 run の開始時刻（ms）。経過時間の表示用 */
  runStartedAtByTask: Record<string, number | null>;
  /** 直近の操作エラー（送信失敗・履歴取得失敗など）。エージェント自身のエラーはイベントとして表示する */
  errorByTask: Record<string, AppError | null>;
  /** 送信中（invoke 完了 or user_message 受信まで）のメッセージ */
  pendingByTask: Record<string, PendingMessage | null>;
  /** 履歴取得の状態 */
  historyStatusByTask: Record<string, "loading" | "loaded" | "error">;

  applyEvent(e: AgentEventEnvelope): void;
  applyEvents(list: AgentEventEnvelope[]): void;
  loadHistory(taskId: string): Promise<void>;
  send(taskId: string, text: string): Promise<boolean>;
  cancel(taskId: string): Promise<void>;
  reset(taskId: string): Promise<void>;
  clearError(taskId: string): void;
}

type Draft = Pick<
  AgentState,
  "eventsByTask" | "runningByTask" | "runStartedAtByTask" | "pendingByTask"
>;

const EMPTY: AgentEventEnvelope[] = [];

/** seq 昇順を保ったまま、未知の seq だけを差し込む。変化がなければ同じ配列を返す */
export function mergeEvents(list: AgentEventEnvelope[], incoming: AgentEventEnvelope[]): AgentEventEnvelope[] {
  if (incoming.length === 0) return list;
  const known = new Set(list.map((e) => e.seq));
  const fresh: AgentEventEnvelope[] = [];
  for (const e of incoming) {
    if (known.has(e.seq)) continue;
    known.add(e.seq);
    fresh.push(e);
  }
  if (fresh.length === 0) return list;
  const last = list.length ? list[list.length - 1].seq : -Infinity;
  fresh.sort((a, b) => a.seq - b.seq);
  // ライブイベントはほぼ常に末尾追加なのでソートを省く
  if (fresh[0].seq > last) return [...list, ...fresh];
  return [...list, ...fresh].sort((a, b) => a.seq - b.seq);
}

/** イベント列から「最後の run が終わっていないか」を判定する */
export function isRunOpen(list: AgentEventEnvelope[]): { open: boolean; runId: string | null; startedAt: number | null } {
  for (let i = list.length - 1; i >= 0; i--) {
    const e = list[i];
    if (e.event.type === "run_finished") return { open: false, runId: e.runId, startedAt: null };
    if (e.event.type === "user_message") {
      const t = Date.parse(e.timestamp);
      return { open: true, runId: e.runId, startedAt: Number.isNaN(t) ? null : t };
    }
  }
  return { open: false, runId: null, startedAt: null };
}

function hasRunFinished(list: AgentEventEnvelope[], runId: string): boolean {
  return list.some((e) => e.runId === runId && e.event.type === "run_finished");
}

/** イベント群を draft に反映する（applyEvent / applyEvents / loadHistory 共通） */
function reduceEvents(s: Draft, incoming: AgentEventEnvelope[]): Partial<Draft> | null {
  const byTask = new Map<string, AgentEventEnvelope[]>();
  for (const e of incoming) {
    const arr = byTask.get(e.taskId);
    if (arr) arr.push(e);
    else byTask.set(e.taskId, [e]);
  }

  let changed = false;
  const eventsByTask = { ...s.eventsByTask };
  const runningByTask = { ...s.runningByTask };
  const runStartedAtByTask = { ...s.runStartedAtByTask };
  const pendingByTask = { ...s.pendingByTask };

  for (const [taskId, evs] of byTask) {
    const prev = s.eventsByTask[taskId] ?? EMPTY;
    const next = mergeEvents(prev, evs);
    if (next === prev) continue;
    changed = true;
    eventsByTask[taskId] = next;

    // 新しく入ったイベントのうち seq 最大のものが実行状態を決める
    const run = isRunOpen(next);
    const newest = evs.reduce((a, b) => (b.seq > a.seq ? b : a));
    const isTail = newest.seq === next[next.length - 1].seq;
    if (isTail) {
      if (newest.event.type === "run_finished") {
        runningByTask[taskId] = false;
        runStartedAtByTask[taskId] = null;
      } else if (run.open) {
        runningByTask[taskId] = true;
        runStartedAtByTask[taskId] = runStartedAtByTask[taskId] ?? run.startedAt ?? Date.now();
      }
    }
    if (evs.some((e) => e.event.type === "user_message" || e.event.type === "run_finished")) {
      pendingByTask[taskId] = null;
    }
  }
  return changed ? { eventsByTask, runningByTask, runStartedAtByTask, pendingByTask } : null;
}

/** 同じタスクの loadHistory が重ならないようにする */
const inflightHistory = new Map<string, Promise<void>>();

export const useAgentStore = create<AgentState>((set, get) => ({
  eventsByTask: {},
  runningByTask: {},
  runStartedAtByTask: {},
  errorByTask: {},
  pendingByTask: {},
  historyStatusByTask: {},

  applyEvent: (e) => get().applyEvents([e]),

  applyEvents: (list) =>
    set((s) => {
      const patch = reduceEvents(s, list);
      return patch ?? s;
    }),

  loadHistory: (taskId) => {
    const inflight = inflightHistory.get(taskId);
    if (inflight) return inflight;

    const p = (async () => {
      set((s) => ({
        historyStatusByTask: {
          ...s.historyStatusByTask,
          // 既に表示済みなら再取得中もローディング表示にしない
          [taskId]: s.historyStatusByTask[taskId] === "loaded" ? "loaded" : "loading",
        },
      }));
      try {
        // 全件取得して seq でマージする（部分取得だと途中の欠落を埋められないため）
        const [history, state] = await Promise.all([
          agentApi.getAgentHistory(taskId),
          agentApi.getAgentRunState(taskId),
        ]);
        set((s) => {
          const patch = reduceEvents(s, history) ?? {};
          const events = patch.eventsByTask?.[taskId] ?? s.eventsByTask[taskId] ?? EMPTY;
          // run state 取得後に run_finished がライブで届いている場合があるので、イベント側を優先する
          const running = state.running && !(state.runId && hasRunFinished(events, state.runId));
          const startedAt = running
            ? (patch.runStartedAtByTask?.[taskId] ?? s.runStartedAtByTask[taskId] ?? isRunOpen(events).startedAt ?? Date.now())
            : null;
          return {
            ...patch,
            runningByTask: { ...(patch.runningByTask ?? s.runningByTask), [taskId]: running },
            runStartedAtByTask: { ...(patch.runStartedAtByTask ?? s.runStartedAtByTask), [taskId]: startedAt },
            historyStatusByTask: { ...s.historyStatusByTask, [taskId]: "loaded" },
          };
        });
      } catch (e) {
        set((s) => ({
          errorByTask: { ...s.errorByTask, [taskId]: toAppError(e) },
          historyStatusByTask: { ...s.historyStatusByTask, [taskId]: "error" },
        }));
      } finally {
        inflightHistory.delete(taskId);
      }
    })();
    inflightHistory.set(taskId, p);
    return p;
  },

  send: async (taskId, text) => {
    const body = text.trim();
    if (!body) return false;
    if (get().runningByTask[taskId] || get().pendingByTask[taskId]) return false;
    set((s) => ({
      errorByTask: { ...s.errorByTask, [taskId]: null },
      pendingByTask: { ...s.pendingByTask, [taskId]: { text: body, sentAt: Date.now() } },
    }));
    try {
      const info = await agentApi.sendAgentMessage({ taskId, text: body });
      set((s) => {
        // 応答より先に run_finished まで届いていれば実行中にしない
        if (hasRunFinished(s.eventsByTask[taskId] ?? EMPTY, info.runId)) return s;
        return {
          runningByTask: { ...s.runningByTask, [taskId]: true },
          runStartedAtByTask: {
            ...s.runStartedAtByTask,
            [taskId]: s.runStartedAtByTask[taskId] ?? s.pendingByTask[taskId]?.sentAt ?? Date.now(),
          },
        };
      });
      return true;
    } catch (e) {
      set((s) => ({
        errorByTask: { ...s.errorByTask, [taskId]: toAppError(e) },
        pendingByTask: { ...s.pendingByTask, [taskId]: null },
      }));
      return false;
    }
  },

  cancel: async (taskId) => {
    try {
      await agentApi.cancelAgentRun(taskId);
    } catch (e) {
      set((s) => ({ errorByTask: { ...s.errorByTask, [taskId]: toAppError(e) } }));
    }
  },

  reset: async (taskId) => {
    try {
      await agentApi.resetAgentSession(taskId);
      set((s) => ({
        eventsByTask: { ...s.eventsByTask, [taskId]: [] },
        runningByTask: { ...s.runningByTask, [taskId]: false },
        runStartedAtByTask: { ...s.runStartedAtByTask, [taskId]: null },
        pendingByTask: { ...s.pendingByTask, [taskId]: null },
        errorByTask: { ...s.errorByTask, [taskId]: null },
      }));
    } catch (e) {
      set((s) => ({ errorByTask: { ...s.errorByTask, [taskId]: toAppError(e) } }));
    }
  },

  clearError: (taskId) => set((s) => ({ errorByTask: { ...s.errorByTask, [taskId]: null } })),
}));

let bridgeStarted = false;

/** `agent://event` の購読を開始する（多重呼び出しは無視） */
export function initAgentEventBridge() {
  if (bridgeStarted) return;
  bridgeStarted = true;
  // 同一フレーム内に大量に届く stdout をまとめて反映し、再描画回数を抑える
  let queue: AgentEventEnvelope[] = [];
  let scheduled = false;
  const flush = () => {
    scheduled = false;
    const batch = queue;
    queue = [];
    useAgentStore.getState().applyEvents(batch);
  };
  agentApi
    .onAgentEvent((e) => {
      queue.push(e);
      if (!scheduled) {
        scheduled = true;
        // rAF はウィンドウ非表示中に止まるため setTimeout を使う
        setTimeout(flush, 16);
      }
    })
    .catch((err) => {
      bridgeStarted = false;
      console.error("agent://event の購読に失敗しました", err);
    });
}
