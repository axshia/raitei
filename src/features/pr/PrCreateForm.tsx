/**
 * PR 作成フォーム（担当: WS-H）。
 * 送信すると Rust 側でブランチを push（upstream 設定）してから `gh pr create` する。
 */
import { useState, type FormEvent } from "react";
import type { Task } from "../../api";
import { usePrStore } from "../../store/prStore";

export function PrCreateForm({ task }: { task: Task }) {
  const creating = usePrStore((s) => s.prOpByTask[task.id] === "create");
  const busy = usePrStore((s) => !!s.prOpByTask[task.id]);
  const createPr = usePrStore((s) => s.createPr);

  const [title, setTitle] = useState(task.title);
  const [body, setBody] = useState("");
  const [base, setBase] = useState(task.baseBranch);
  const [draft, setDraft] = useState(false);

  const canSubmit = title.trim() !== "" && base.trim() !== "" && !busy;

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    const ok = await createPr({
      taskId: task.id,
      title: title.trim(),
      body,
      base: base.trim() === task.baseBranch ? null : base.trim(),
      draft,
    });
    if (ok) setBody("");
  };

  return (
    <form className="wsh-section" onSubmit={onSubmit}>
      <div className="wsh-section-title">PR を作成</div>
      <p className="muted" style={{ margin: 0 }}>
        <span className="wsh-mono">{task.branch}</span> を push してから PR を作成します。
      </p>
      <label className="wsh-field">
        <span>タイトル</span>
        <input value={title} onChange={(e) => setTitle(e.target.value)} disabled={creating} required />
      </label>
      <label className="wsh-field">
        <span>本文（Markdown）</span>
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          disabled={creating}
          placeholder="変更の概要・確認方法など"
        />
      </label>
      <label className="wsh-field">
        <span>マージ先（base）</span>
        <input className="wsh-mono" value={base} onChange={(e) => setBase(e.target.value)} disabled={creating} required />
      </label>
      <label className="wsh-check">
        <input type="checkbox" checked={draft} onChange={(e) => setDraft(e.target.checked)} disabled={creating} />
        ドラフトとして作成
      </label>
      <div className="wsh-actions">
        <button type="submit" className="wsh-primary" disabled={!canSubmit}>
          {creating ? "push して作成中…" : draft ? "ドラフト PR を作成" : "PR を作成"}
        </button>
      </div>
    </form>
  );
}
