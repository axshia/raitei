import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { call } from "./invoke";
import {
  AGENT_EVENT,
  type AgentEventEnvelope,
  type AgentRunInfo,
  type AgentRunState,
  type SendMessageRequest,
} from "./types";

export const sendAgentMessage = (req: SendMessageRequest) => call<AgentRunInfo>("send_agent_message", { req });
export const cancelAgentRun = (taskId: string) => call<void>("cancel_agent_run", { taskId });
export const getAgentHistory = (taskId: string, afterSeq?: number) =>
  call<AgentEventEnvelope[]>("get_agent_history", { taskId, afterSeq: afterSeq ?? null });
export const getAgentRunState = (taskId: string) => call<AgentRunState>("get_agent_run_state", { taskId });
/** 会話リセット（session_id 破棄 + 履歴削除） */
export const resetAgentSession = (taskId: string) => call<void>("reset_agent_session", { taskId });

/** 全タスクのエージェントイベントを購読する（アプリで 1 回だけ呼ぶ想定） */
export const onAgentEvent = (handler: (e: AgentEventEnvelope) => void): Promise<UnlistenFn> =>
  listen<AgentEventEnvelope>(AGENT_EVENT, (ev) => handler(ev.payload));
