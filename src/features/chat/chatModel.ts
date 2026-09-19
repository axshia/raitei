/**
 * AgentEvent 列 → チャット表示項目への変換（担当: WS-G）。副作用なしの純粋関数だけを置く。
 *
 * - tool_use と tool_result を id で対応付けて 1 項目にまとめる
 * - 連続する stderr 行は 1 項目にまとめる（既定で折りたたみ）
 * - session_started はモデル名の小さな注記にする
 * - run_finished は正常終了なら表示せず、キャンセル・異常終了のときだけ表示する
 */
import type { AgentEventEnvelope, AgentKind } from "../../api";

export type ChatItem =
  | { kind: "user"; key: string; text: string; timestamp: string; pending?: boolean }
  | { kind: "assistant"; key: string; text: string; agent: AgentKind }
  | { kind: "thinking"; key: string; text: string }
  | {
      kind: "tool";
      key: string;
      id: string;
      name: string;
      input: unknown;
      /** 結果未着なら null */
      result: { output: string; isError: boolean } | null;
      /** 結果が届かないまま run が終わった */
      orphaned: boolean;
    }
  | {
      kind: "result";
      key: string;
      isError: boolean;
      text: string | null;
      durationMs: number | null;
      costUsd: number | null;
    }
  | { kind: "error"; key: string; message: string }
  | { kind: "stderr"; key: string; lines: string[] }
  | { kind: "session"; key: string; sessionId: string; model: string | null; agent: AgentKind }
  | { kind: "runEnd"; key: string; cancelled: boolean; exitCode: number | null };

export function buildChatItems(events: AgentEventEnvelope[]): ChatItem[] {
  const items: ChatItem[] = [];
  /** runId + tool id → items 内のインデックス。項目は追記か末尾置換しかしないのでインデックスは不変 */
  const tools = new Map<string, number>();
  /** run ごとの結果未着ツール（インデックス） */
  const openToolsByRun = new Map<string, Set<number>>();
  /** assistant_text を出した run。result.text は最後の本文の再掲なので、本文があれば表示しない */
  const runsWithText = new Set<string>();

  for (const env of events) {
    const ev = env.event;
    const key = String(env.seq);
    switch (ev.type) {
      case "user_message":
        items.push({ kind: "user", key, text: ev.text, timestamp: env.timestamp });
        break;
      case "session_started":
        items.push({ kind: "session", key, sessionId: ev.session_id, model: ev.model, agent: env.agent });
        break;
      case "assistant_text": {
        if (!ev.text) break;
        const prev = items[items.length - 1];
        // claude は content ブロック単位、codex は item 単位で届く。連続する本文は 1 つの吹き出しにまとめる
        if (prev && prev.kind === "assistant") {
          items[items.length - 1] = { ...prev, text: joinParagraphs(prev.text, ev.text) };
        } else {
          items.push({ kind: "assistant", key, text: ev.text, agent: env.agent });
        }
        runsWithText.add(env.runId);
        break;
      }
      case "thinking":
        if (ev.text) items.push({ kind: "thinking", key, text: ev.text });
        break;
      case "tool_use": {
        const index = items.length;
        items.push({ kind: "tool", key, id: ev.id, name: ev.name, input: ev.input, result: null, orphaned: false });
        tools.set(`${env.runId}\u0000${ev.id}`, index);
        const open = openToolsByRun.get(env.runId) ?? new Set<number>();
        open.add(index);
        openToolsByRun.set(env.runId, open);
        break;
      }
      case "tool_result": {
        const index = tools.get(`${env.runId}\u0000${ev.tool_use_id}`);
        const result = { output: ev.output, isError: ev.is_error };
        const item = index === undefined ? undefined : items[index];
        if (index !== undefined && item?.kind === "tool") {
          items[index] = { ...item, result };
          openToolsByRun.get(env.runId)?.delete(index);
        } else {
          // 対応する tool_use がない結果も捨てずに出す
          items.push({ kind: "tool", key, id: ev.tool_use_id, name: "(不明なツール)", input: null, result, orphaned: false });
        }
        break;
      }
      case "result": {
        // 失敗時の text はエラー内容なので常に出す。成功時は本文がなかった run だけ出す
        const text = ev.text?.trim() && (ev.is_error || !runsWithText.has(env.runId)) ? ev.text : null;
        items.push({
          kind: "result",
          key,
          isError: ev.is_error,
          text,
          durationMs: ev.duration_ms,
          costUsd: ev.cost_usd,
        });
        break;
      }
      case "error":
        items.push({ kind: "error", key, message: ev.message });
        break;
      case "stderr": {
        const prev = items[items.length - 1];
        if (prev && prev.kind === "stderr") {
          items[items.length - 1] = { ...prev, lines: [...prev.lines, ev.line] };
        } else {
          items.push({ kind: "stderr", key, lines: [ev.line] });
        }
        break;
      }
      case "run_finished": {
        for (const index of openToolsByRun.get(env.runId) ?? []) {
          const current = items[index];
          if (current.kind === "tool" && !current.result) items[index] = { ...current, orphaned: true };
        }
        openToolsByRun.delete(env.runId);
        if (ev.cancelled || (ev.exit_code !== null && ev.exit_code !== 0)) {
          items.push({ kind: "runEnd", key, cancelled: ev.cancelled, exitCode: ev.exit_code });
        }
        break;
      }
    }
  }
  return items;
}

function joinParagraphs(a: string, b: string): string {
  if (a.endsWith("\n\n") || b.startsWith("\n")) return a + b;
  return `${a.replace(/\n+$/, "")}\n\n${b}`;
}

// 同じ配列参照なら変換結果を使い回す（zustand のイベント配列はイミュータブル）
const cache = new WeakMap<AgentEventEnvelope[], ChatItem[]>();

export function selectChatItems(events: AgentEventEnvelope[]): ChatItem[] {
  let items = cache.get(events);
  if (!items) {
    items = buildChatItems(events);
    cache.set(events, items);
  }
  return items;
}

// ---------- ツール表示用の要約 ----------

const PATH_KEYS = ["file_path", "path", "notebook_path"];

/** ツール呼び出しの 1 行要約（折りたたみ時の見出しに使う） */
export function summarizeToolInput(name: string, input: unknown): string {
  if (input == null) return "";
  if (typeof input === "string") return firstLine(input);
  if (typeof input !== "object") return String(input);
  const o = input as Record<string, unknown>;

  if (typeof o.command === "string") return firstLine(o.command);
  if (Array.isArray(o.command)) return firstLine(o.command.map(String).join(" "));
  for (const k of PATH_KEYS) if (typeof o[k] === "string") return o[k] as string;
  if (typeof o.pattern === "string") return `${o.pattern}${typeof o.path === "string" ? ` in ${o.path}` : ""}`;
  if (typeof o.url === "string") return o.url;
  if (typeof o.query === "string") return o.query;
  if (typeof o.description === "string") return firstLine(o.description);
  if (typeof o.prompt === "string") return firstLine(o.prompt);
  if (Array.isArray(o.changes)) {
    // codex file_change: { changes: [{ path, kind }] }
    const paths = o.changes
      .map((c) => (c && typeof c === "object" && typeof (c as { path?: unknown }).path === "string" ? (c as { path: string }).path : null))
      .filter((p): p is string => !!p);
    if (paths.length) return paths.length > 2 ? `${paths.slice(0, 2).join(", ")} ほか ${paths.length - 2} 件` : paths.join(", ");
  }
  if (name === "TodoWrite" && Array.isArray(o.todos)) return `${o.todos.length} 件の TODO`;
  const keys = Object.keys(o);
  return keys.length ? keys.slice(0, 3).join(", ") : "";
}

/** 展開時に表示する入力。command や本文は生のまま、それ以外は整形 JSON */
export function formatToolInput(input: unknown): string {
  if (input == null) return "";
  if (typeof input === "string") return input;
  if (typeof input === "object" && !Array.isArray(input)) {
    const o = input as Record<string, unknown>;
    const keys = Object.keys(o);
    if (keys.length === 1 && typeof o.command === "string") return o.command;
  }
  try {
    return JSON.stringify(input, null, 2);
  } catch {
    return String(input);
  }
}

function firstLine(s: string): string {
  const line = s.split("\n").find((l) => l.trim()) ?? "";
  return line.length > 160 ? `${line.slice(0, 160)}…` : line;
}

export function formatDuration(ms: number): string {
  const sec = Math.floor(ms / 1000);
  if (sec < 60) return `${sec}秒`;
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  if (m < 60) return `${m}分${s.toString().padStart(2, "0")}秒`;
  return `${Math.floor(m / 60)}時間${(m % 60).toString().padStart(2, "0")}分`;
}
