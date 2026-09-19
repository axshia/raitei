/**
 * エージェント会話ストア（担当: WS-G）。
 *
 * - `initAgentEventBridge()` をアプリ起動時に 1 回呼び、`agent://event` を購読して applyEvent する
 * - タブを開いたら `loadHistory(taskId)` で過去分を取得（seq で重複排除）
 * - 表示用のメッセージ整形（tool_use と tool_result の対応付け等）は WS-G がセレクタとして追加する
 */
import { create } from "zustand";
import { agentApi, toAppError } from "../api";
import type { AgentEventEnvelope, AppError } from "../api";

interface AgentState {
  eventsByTask: Record<string, AgentEventEnvelope[]>;
  runningByTask: Record<string, boolean>;
  errorByTask: Record<string, AppError | null>;

  applyEvent(e: AgentEventEnvelope): void;
  loadHistory(taskId: string): Promise<void>;
  send(taskId: string, text: string): Promise<void>;
  cancel(taskId: string): Promise<void>;
  reset(taskId: string): Promise<void>;
}

export const useAgentStore = create<AgentState>((set, get) => ({
  eventsByTask: {},
  runningByTask: {},
  errorByTask: {},

  applyEvent: (e) =>
    set((s) => {
      const list = s.eventsByTask[e.taskId] ?? [];
      if (list.some((x) => x.seq === e.seq)) return s;
      const next = [...list, e].sort((a, b) => a.seq - b.seq);
      const running =
        e.event.type === "run_finished" ? false : e.event.type === "user_message" ? true : s.runningByTask[e.taskId];
      return {
        eventsByTask: { ...s.eventsByTask, [e.taskId]: next },
        runningByTask: { ...s.runningByTask, [e.taskId]: !!running },
      };
    }),

  loadHistory: async (taskId) => {
    try {
      const [history, state] = await Promise.all([agentApi.getAgentHistory(taskId), agentApi.getAgentRunState(taskId)]);
      history.forEach(get().applyEvent);
      set((s) => ({ runningByTask: { ...s.runningByTask, [taskId]: state.running } }));
    } catch (e) {
      set((s) => ({ errorByTask: { ...s.errorByTask, [taskId]: toAppError(e) } }));
    }
  },

  send: async (taskId, text) => {
    try {
      set((s) => ({ errorByTask: { ...s.errorByTask, [taskId]: null } }));
      await agentApi.sendAgentMessage({ taskId, text });
    } catch (e) {
      set((s) => ({ errorByTask: { ...s.errorByTask, [taskId]: toAppError(e) } }));
    }
  },

  cancel: async (taskId) => {
    await agentApi.cancelAgentRun(taskId);
  },

  reset: async (taskId) => {
    await agentApi.resetAgentSession(taskId);
    set((s) => ({ eventsByTask: { ...s.eventsByTask, [taskId]: [] } }));
  },
}));

let bridgeStarted = false;

/** `agent://event` の購読を開始する（多重呼び出しは無視） */
export function initAgentEventBridge() {
  if (bridgeStarted) return;
  bridgeStarted = true;
  void agentApi.onAgentEvent((e) => useAgentStore.getState().applyEvent(e));
}
