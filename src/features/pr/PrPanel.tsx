/**
 * PR パネル（担当: WS-H）: PR 未作成なら作成フォーム、作成済みなら状態（CI / レビュー / mergeable / コンフリクト）とマージ操作。
 * タブがアクティブな間は 30 秒ごとに状態をポーリングする（usePrPolling）。
 */
import { useEffect, useState } from "react";
import { useProjectStore } from "../../store/projectStore";
import { usePrStore } from "../../store/prStore";
import { ErrorNotice, formatAgo, NoticeView } from "./common";
import { PrCreateForm } from "./PrCreateForm";
import { PrMergeControls } from "./PrMergeControls";
import { PrChecks, PrHeader, PrMergeability, PrReviews } from "./PrStatusView";
import { usePrPolling } from "./usePrPolling";
import "./pr.css";

/** 「n 秒前」表示を更新するための時計 */
function useNow(intervalMs: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(t);
  }, [intervalMs]);
  return now;
}

export function PrPanel({ taskId }: { taskId: string }) {
  const task = useProjectStore((s) => s.findTask(taskId));
  const pr = usePrStore((s) => s.prByTask[taskId]);
  const fetchedAt = usePrStore((s) => s.prFetchedAtByTask[taskId]);
  const op = usePrStore((s) => s.prOpByTask[taskId] ?? null);
  const error = usePrStore((s) => s.prErrorByTask[taskId]);
  const notice = usePrStore((s) => s.prNoticeByTask[taskId]);
  const refreshPr = usePrStore((s) => s.refreshPr);
  const clearPrError = usePrStore((s) => s.clearPrError);
  const now = useNow(10_000);

  usePrPolling(taskId);

  if (!task) return null;

  return (
    <div className="wsh-panel">
      <div className="wsh-toolbar">
        <button disabled={!!op} onClick={() => refreshPr(taskId)}>
          {op === "refresh" ? "更新中…" : "更新"}
        </button>
        <span className="wsh-spacer" />
        <span className="muted" style={{ fontSize: 11 }} title={fetchedAt ? new Date(fetchedAt).toLocaleString() : ""}>
          {formatAgo(fetchedAt, now)}に取得
        </span>
      </div>

      <ErrorNotice error={error} onClose={() => clearPrError(taskId)} />
      <NoticeView notice={notice} />

      {pr === undefined && !error && <p className="muted">PR の状態を取得しています…</p>}
      {pr === null && <PrCreateForm key={taskId} task={task} />}
      {pr && (
        <>
          <PrHeader pr={pr} />
          <PrMergeability pr={pr} taskId={taskId} />
          <PrChecks checks={pr.checks} />
          <PrReviews pr={pr} />
          <PrMergeControls key={pr.number} taskId={taskId} pr={pr} />
        </>
      )}
    </div>
  );
}
