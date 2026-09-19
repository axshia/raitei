/**
 * エージェントとのチャット（担当: WS-G）。
 * 仮実装: イベントを type ごとに 1 行表示 + 送信欄。WS-G がメッセージ整形・ツール折りたたみ・Markdown 等を実装する。
 */
import { useEffect, useState } from "react";
import { useAgentStore } from "../../store/agentStore";

export function ChatPanel({ taskId }: { taskId: string }) {
  const events = useAgentStore((s) => s.eventsByTask[taskId] ?? []);
  const running = useAgentStore((s) => !!s.runningByTask[taskId]);
  const error = useAgentStore((s) => s.errorByTask[taskId]);
  const { loadHistory, send, cancel } = useAgentStore();
  const [text, setText] = useState("");

  useEffect(() => {
    void loadHistory(taskId);
  }, [taskId, loadHistory]);

  const submit = async () => {
    if (!text.trim()) return;
    const t = text;
    setText("");
    await send(taskId, t);
  };

  return (
    <section style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div style={{ flex: 1, overflowY: "auto", padding: 12, fontFamily: "var(--mono)" }}>
        {events.map((e) => (
          <div key={e.seq} style={{ marginBottom: 4, whiteSpace: "pre-wrap" }}>
            <span className="muted">[{e.event.type}]</span> {"text" in e.event ? e.event.text : ""}
            {"message" in e.event ? e.event.message : ""}
          </div>
        ))}
        {error && <div className="error">{error.message}</div>}
      </div>
      <div style={{ display: "flex", gap: 6, padding: 8, borderTop: "1px solid var(--border)" }}>
        <textarea
          style={{ flex: 1, minHeight: 60 }}
          placeholder="エージェントへの指示（⌘+Enter で送信）"
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && e.metaKey) void submit();
          }}
        />
        {running ? <button onClick={() => cancel(taskId)}>停止</button> : <button onClick={submit}>送信</button>}
      </div>
    </section>
  );
}
