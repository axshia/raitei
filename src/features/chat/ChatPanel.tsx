/**
 * エージェントとのチャット（担当: WS-G）。
 *
 * - 表示: ユーザー発言 / アシスタント本文（Markdown）/ ツール呼び出しと結果（折りたたみ）/ 思考・stderr（折りたたみ）/
 *   完了・エラー・キャンセルの注記
 * - タブを開いた（マウントした）ときに `loadHistory` で履歴を復元する。ライブイベントとの重複は store が seq で除く
 * - 非アクティブなタブでも display:none で生きているので、入力中テキストとスクロール位置は保持される
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { useAgentStore } from "../../store/agentStore";
import { ChatItemView } from "./ChatItemView";
import { formatDuration, selectChatItems, type ChatItem } from "./chatModel";
import "./chat.css";

const EMPTY_EVENTS: never[] = [];
/** 最下部からこの距離以内なら「追従中」とみなし、新着で自動スクロールする */
const STICK_THRESHOLD = 48;

export function ChatPanel({ taskId }: { taskId: string }) {
  const events = useAgentStore((s) => s.eventsByTask[taskId] ?? EMPTY_EVENTS);
  const running = useAgentStore((s) => !!s.runningByTask[taskId]);
  const startedAt = useAgentStore((s) => s.runStartedAtByTask[taskId] ?? null);
  const error = useAgentStore((s) => s.errorByTask[taskId] ?? null);
  const pending = useAgentStore((s) => s.pendingByTask[taskId] ?? null);
  const historyStatus = useAgentStore((s) => s.historyStatusByTask[taskId]);
  const loadHistory = useAgentStore((s) => s.loadHistory);
  const send = useAgentStore((s) => s.send);
  const cancel = useAgentStore((s) => s.cancel);
  const reset = useAgentStore((s) => s.reset);
  const clearError = useAgentStore((s) => s.clearError);

  const [text, setText] = useState("");
  const [cancelling, setCancelling] = useState(false);
  const busy = running || !!pending;

  useEffect(() => {
    void loadHistory(taskId);
  }, [taskId, loadHistory]);

  useEffect(() => {
    if (!running) setCancelling(false);
  }, [running]);

  const items: ChatItem[] = selectChatItems(events);
  const shownItems: ChatItem[] = pending
    ? [...items, { kind: "user", key: "pending", text: pending.text, timestamp: "", pending: true }]
    : items;

  // ---------- スクロール追従 ----------
  const logRef = useRef<HTMLDivElement>(null);
  const stickRef = useRef(true);
  const [atBottom, setAtBottom] = useState(true);

  const onScroll = useCallback(() => {
    const el = logRef.current;
    if (!el || el.clientHeight === 0) return; // display:none 中は判定しない
    const near = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_THRESHOLD;
    stickRef.current = near;
    setAtBottom(near);
  }, []);

  const scrollToBottom = useCallback(() => {
    const el = logRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    stickRef.current = true;
    setAtBottom(true);
  }, []);

  useLayoutEffect(() => {
    if (stickRef.current) {
      const el = logRef.current;
      if (el) el.scrollTop = el.scrollHeight;
    }
  }, [shownItems.length, events, running]);

  // ---------- 操作 ----------
  const submit = async () => {
    const body = text.trim();
    if (!body || busy) return;
    setText("");
    stickRef.current = true;
    const ok = await send(taskId, body);
    // 失敗したら入力を戻す（ユーザーがその間に新しく入力していれば上書きしない）
    if (!ok) setText((cur) => (cur ? cur : body));
  };

  const onCancel = async () => {
    setCancelling(true);
    await cancel(taskId);
  };

  const onReset = async () => {
    const ok = await confirm("会話履歴とエージェントのセッションを破棄して、新しい会話を始めますか？", {
      title: "会話をリセット",
      kind: "warning",
      okLabel: "リセット",
      cancelLabel: "キャンセル",
    }).catch(() => window.confirm("会話履歴とエージェントのセッションを破棄しますか？"));
    if (ok) await reset(taskId);
  };

  return (
    <section className="chat-panel">
      <div className="chat-toolbar">
        <RunStatus running={running} pending={!!pending} startedAt={startedAt} cancelling={cancelling} />
        <div className="chat-toolbar-actions">
          <button type="button" onClick={onReset} disabled={busy || events.length === 0} title="セッションと履歴を破棄">
            会話をリセット
          </button>
        </div>
      </div>

      <div className="chat-log" ref={logRef} onScroll={onScroll} aria-live="polite">
        {historyStatus === "loading" && items.length === 0 && <div className="chat-placeholder">履歴を読み込み中…</div>}
        {historyStatus !== "loading" && shownItems.length === 0 && (
          <div className="chat-placeholder">
            エージェントへの指示を入力してください。
            <br />
            <span className="muted">⌘+Enter で送信します。</span>
          </div>
        )}
        {shownItems.map((item) => (
          <ChatItemView key={item.key} item={item} running={running} />
        ))}
        {running && (
          <div className="chat-working">
            <span className="chat-spinner" />
            {cancelling ? "停止しています…" : "エージェントが作業中…"}
          </div>
        )}
      </div>

      {!atBottom && (
        <button type="button" className="chat-jump" onClick={scrollToBottom}>
          ↓ 最新へ
        </button>
      )}

      {error && (
        <div className="chat-banner" role="alert">
          <span className="chat-banner-kind">{ERROR_KIND_LABEL[error.kind] ?? error.kind}</span>
          <span className="chat-banner-msg">{error.message}</span>
          {historyStatus === "error" && (
            <button type="button" onClick={() => void loadHistory(taskId)}>
              再読み込み
            </button>
          )}
          <button type="button" className="chat-link-button" onClick={() => clearError(taskId)} aria-label="閉じる">
            ✕
          </button>
        </div>
      )}

      <div className="chat-input">
        <textarea
          value={text}
          placeholder={busy ? "実行中です。完了または停止後に送信できます" : "エージェントへの指示（⌘+Enter で送信）"}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && !e.nativeEvent.isComposing) {
              e.preventDefault();
              void submit();
            }
          }}
        />
        {running ? (
          <button type="button" className="chat-stop" onClick={onCancel} disabled={cancelling}>
            {cancelling ? "停止中…" : "停止"}
          </button>
        ) : (
          <button type="button" className="chat-send" onClick={submit} disabled={busy || !text.trim()}>
            送信
          </button>
        )}
      </div>
    </section>
  );
}

const ERROR_KIND_LABEL: Record<string, string> = {
  notImplemented: "未実装",
  notFound: "見つかりません",
  invalidInput: "入力エラー",
  git: "git",
  gh: "gh",
  agent: "エージェント",
  command: "コマンド",
  db: "DB",
  io: "I/O",
};

function RunStatus({
  running,
  pending,
  startedAt,
  cancelling,
}: {
  running: boolean;
  pending: boolean;
  startedAt: number | null;
  cancelling: boolean;
}) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [running]);

  if (running) {
    return (
      <span className="chat-status is-running">
        <span className="chat-dot" />
        {cancelling ? "停止中" : "実行中"}
        {startedAt != null && ` · ${formatDuration(Math.max(0, now - startedAt))}`}
      </span>
    );
  }
  if (pending) return <span className="chat-status is-running">送信中…</span>;
  return <span className="chat-status">待機中</span>;
}
