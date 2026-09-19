/** チャット 1 項目の描画（担当: WS-G） */
import { memo, useState } from "react";
import { Markdown } from "./Markdown";
import { formatDuration, formatToolInput, summarizeToolInput, type ChatItem } from "./chatModel";

export const ChatItemView = memo(function ChatItemView({ item, running }: { item: ChatItem; running: boolean }) {
  switch (item.kind) {
    case "user":
      return (
        <div className={`chat-row chat-user${item.pending ? " is-pending" : ""}`}>
          <div className="chat-bubble">{item.text}</div>
          {item.pending && <div className="chat-meta">送信中…</div>}
        </div>
      );
    case "assistant":
      return (
        <div className="chat-row chat-assistant">
          <div className="chat-role">{item.agent}</div>
          <Markdown text={item.text} />
        </div>
      );
    case "thinking":
      return (
        <details className="chat-fold chat-thinking">
          <summary>
            <span className="chat-fold-title">思考</span>
            <span className="chat-fold-summary">{firstLine(item.text)}</span>
          </summary>
          <div className="chat-fold-body chat-thinking-body">{item.text}</div>
        </details>
      );
    case "tool":
      return <ToolCall item={item} running={running} />;
    case "result":
      return <ResultLine item={item} />;
    case "error":
      return (
        <div className="chat-row chat-error is-warn">
          <span className="chat-error-label">エージェントの警告</span>
          <span className="chat-pre">{item.message}</span>
        </div>
      );
    case "stderr":
      return (
        <details className="chat-fold chat-stderr">
          <summary>
            <span className="chat-fold-title">stderr</span>
            <span className="chat-fold-summary">
              {item.lines.length} 行 · {firstLine(item.lines[item.lines.length - 1] ?? "")}
            </span>
          </summary>
          <pre className="chat-fold-body">{item.lines.join("\n")}</pre>
        </details>
      );
    case "session":
      return (
        <div className="chat-note" title={`session: ${item.sessionId}`}>
          {item.agent}
          {item.model ? ` · ${item.model}` : ""} · セッション {item.sessionId.slice(0, 8)}
        </div>
      );
    case "runEnd":
      return (
        <div className={`chat-note ${item.cancelled ? "chat-note-warn" : "chat-note-danger"}`}>
          {item.cancelled ? "実行をキャンセルしました" : `プロセスが終了コード ${item.exitCode} で終了しました`}
        </div>
      );
  }
});

type ToolItem = Extract<ChatItem, { kind: "tool" }>;

/** 出力がこの行数を超えたら「すべて表示」で展開する */
const OUTPUT_PREVIEW_LINES = 40;

function ToolCall({ item, running }: { item: ToolItem; running: boolean }) {
  const [showAll, setShowAll] = useState(false);
  const summary = summarizeToolInput(item.name, item.input);
  const input = formatToolInput(item.input);
  const status = item.result
    ? item.result.isError
      ? "error"
      : "ok"
    : item.orphaned || !running
      ? "orphaned"
      : "running";
  const output = item.result?.output ?? "";
  const lines = output.split("\n");
  const truncated = !showAll && lines.length > OUTPUT_PREVIEW_LINES;
  const shown = truncated ? lines.slice(0, OUTPUT_PREVIEW_LINES).join("\n") : output;

  return (
    <details className={`chat-fold chat-tool is-${status}`}>
      <summary>
        <span className={`chat-tool-status is-${status}`} aria-label={STATUS_LABEL[status]} title={STATUS_LABEL[status]}>
          {status === "running" ? <span className="chat-spinner" /> : STATUS_ICON[status]}
        </span>
        <span className="chat-fold-title">{item.name}</span>
        <span className="chat-fold-summary">{summary}</span>
      </summary>
      <div className="chat-fold-body">
        {input && (
          <>
            <div className="chat-tool-label">入力</div>
            <pre>{input}</pre>
          </>
        )}
        <div className="chat-tool-label">{item.result?.isError ? "結果（エラー）" : "結果"}</div>
        {item.result ? (
          output ? (
            <>
              <pre className={item.result.isError ? "is-error" : undefined}>{shown}</pre>
              {truncated && (
                <button type="button" className="chat-link-button" onClick={() => setShowAll(true)}>
                  残り {lines.length - OUTPUT_PREVIEW_LINES} 行を表示
                </button>
              )}
            </>
          ) : (
            <div className="muted">（出力なし）</div>
          )
        ) : (
          <div className="muted">{status === "running" ? "実行中…" : "結果を受け取れませんでした"}</div>
        )}
      </div>
    </details>
  );
}

const STATUS_ICON = { ok: "✓", error: "✕", orphaned: "–", running: "" } as const;
const STATUS_LABEL = { ok: "成功", error: "失敗", orphaned: "結果なし", running: "実行中" } as const;

function ResultLine({ item }: { item: Extract<ChatItem, { kind: "result" }> }) {
  const meta: string[] = [];
  if (item.durationMs != null) meta.push(formatDuration(item.durationMs));
  if (item.costUsd != null) meta.push(`$${item.costUsd.toFixed(4)}`);
  if (item.isError) {
    return (
      <div className="chat-row chat-error" role="alert">
        <span className="chat-error-label">失敗</span>
        <span className="chat-pre">{item.text || "エージェントがエラーで終了しました"}</span>
        {meta.length > 0 && <span className="chat-meta"> · {meta.join(" · ")}</span>}
      </div>
    );
  }
  return (
    <>
      {item.text && (
        <div className="chat-row chat-assistant">
          <Markdown text={item.text} />
        </div>
      )}
      <div className="chat-note">完了{meta.length > 0 ? ` · ${meta.join(" · ")}` : ""}</div>
    </>
  );
}

function firstLine(s: string): string {
  const l = s.split("\n").find((x) => x.trim()) ?? "";
  return l.length > 120 ? `${l.slice(0, 120)}…` : l;
}
