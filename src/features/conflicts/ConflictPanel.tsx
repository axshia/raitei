/**
 * コンフリクト解消パネル（担当: WS-H）:
 * base 取り込み → 競合ファイル一覧 → ファイルごとに ours/theirs/解決済み or AI に依頼 → コミット・push / 中止。
 *
 * AI に依頼した run の完了は agentStore の `run_finished` イベント（runId 一致）で検知し、一覧を取り直す。
 */
import { useEffect, useState } from "react";
import { useAgentStore } from "../../store/agentStore";
import { useProjectStore } from "../../store/projectStore";
import { usePrStore } from "../../store/prStore";
import { ErrorNotice, NoticeView } from "../pr/common";
import { ConflictFileItem } from "./ConflictFileItem";
import "../pr/pr.css";
import "./conflicts.css";

type Step = "idle" | "resolve" | "commit";

function Steps({ step }: { step: Step }) {
  const order: Step[] = ["idle", "resolve", "commit"];
  const idx = order.indexOf(step);
  const names = ["1. base 取り込み", "2. 競合を解消", "3. コミット・push"];
  return (
    <div className="cf-steps">
      {names.map((n, i) => (
        <span key={n} className={i < idx ? "done" : i === idx ? "current" : undefined}>
          {n}
        </span>
      ))}
    </div>
  );
}

/** AI に依頼した run が終わったら一覧を更新する */
function useAgentRunWatcher(taskId: string) {
  const runId = usePrStore((s) => s.agentRunByTask[taskId]?.runId ?? null);
  const finished = useAgentStore((s) =>
    runId ? (s.eventsByTask[taskId] ?? []).some((e) => e.runId === runId && e.event.type === "run_finished") : false,
  );
  useEffect(() => {
    if (runId && finished) void usePrStore.getState().onAgentRunFinished(taskId);
  }, [taskId, runId, finished]);
}

export function ConflictPanel({ taskId }: { taskId: string }) {
  const task = useProjectStore((s) => s.findTask(taskId));
  const state = usePrStore((s) => s.conflictByTask[taskId]);
  const op = usePrStore((s) => s.conflictOpByTask[taskId] ?? null);
  const error = usePrStore((s) => s.conflictErrorByTask[taskId]);
  const notice = usePrStore((s) => s.conflictNoticeByTask[taskId]);
  const agentRun = usePrStore((s) => s.agentRunByTask[taskId] ?? null);
  const agentRunning = useAgentStore((s) => !!s.runningByTask[taskId]);
  const { refreshConflicts, startBaseMerge, askAgentToResolve, completeMerge, abortMerge, clearConflictError } =
    usePrStore.getState();

  const [confirmAbort, setConfirmAbort] = useState(false);

  useAgentRunWatcher(taskId);

  useEffect(() => {
    void refreshConflicts(taskId);
  }, [taskId, refreshConflicts]);

  // マージが終わったら中止確認を閉じる
  useEffect(() => {
    if (!state?.mergeInProgress) setConfirmAbort(false);
  }, [state?.mergeInProgress]);

  if (!task) return null;

  const busy = !!op;
  // エージェントが worktree を触っている間は手動操作を止める
  // （agentRun だけで止めると、イベントが届かない場合に操作不能になるため runningByTask で判定する）
  const locked = busy || agentRunning;
  const files = state?.files ?? [];
  const step: Step = !state?.mergeInProgress ? "idle" : files.length > 0 ? "resolve" : "commit";
  const baseRef = state?.baseRef ?? task.baseBranch;

  return (
    <div className="wsh-panel">
      <div className="wsh-toolbar">
        <button disabled={busy} onClick={() => refreshConflicts(taskId)}>
          {op === "refresh" ? "更新中…" : "状態を更新"}
        </button>
        <span className="wsh-spacer" />
        <span className="muted wsh-mono" style={{ fontSize: 11 }}>
          {task.branch} ← {baseRef}
        </span>
      </div>

      <Steps step={step} />
      <ErrorNotice error={error} onClose={() => clearConflictError(taskId)} />
      <NoticeView notice={notice} />

      {(agentRun || agentRunning) && step !== "idle" && (
        <div className="wsh-notice">
          <span className="tone-accent">●</span> エージェントが作業中です。進捗は左のチャットで確認できます。終わると一覧を自動で更新します。
        </div>
      )}

      {state === undefined && !error && <p className="muted">状態を取得しています…</p>}

      {state && step === "idle" && (
        <div className="wsh-section">
          <div className="wsh-section-title">base の変更を取り込む</div>
          <p style={{ margin: 0 }}>
            <span className="wsh-mono">{task.baseBranch}</span> の最新（origin があれば fetch 後）をこのブランチへ merge
            します。コンフリクトが起きたら、この画面でファイルごとに解消します。
          </p>
          <p className="muted" style={{ margin: 0, fontSize: 12 }}>
            未コミットの変更があると取り込めない場合があります。先にコミットしてください。
          </p>
          <div className="wsh-actions">
            <button className="wsh-primary" disabled={locked} onClick={() => startBaseMerge(taskId)}>
              {op === "start" ? "取り込み中…" : `${task.baseBranch} を取り込む`}
            </button>
          </div>
        </div>
      )}

      {state && step === "resolve" && (
        <div className="wsh-section">
          <div className="wsh-section-title">
            未解決のコンフリクト
            <span className="wsh-badge tone-danger">{files.length} 件</span>
          </div>
          <p className="muted" style={{ margin: 0, fontSize: 12 }}>
            ours = このブランチ（{task.branch}）、theirs = 取り込み元（{baseRef}）
          </p>
          <ul className="wsh-list" style={{ gap: 6 }}>
            {files.map((f) => (
              <ConflictFileItem key={f.path} taskId={taskId} file={f} disabled={locked} />
            ))}
          </ul>
          <div className="wsh-actions">
            <button disabled={locked} onClick={() => askAgentToResolve(taskId)} title="エージェントが編集して git add まで行います（コミットはしません）">
              {op === "agent" ? "依頼中…" : `AI（${task.agent}）に解消を依頼`}
            </button>
          </div>
        </div>
      )}

      {state && step === "commit" && (
        <div className="wsh-section">
          <div className="wsh-section-title">
            <span className="tone-ok">✓</span> すべてのコンフリクトを解消しました
          </div>
          <p style={{ margin: 0 }}>マージコミットを作成します。push すると PR にも反映されます。</p>
          <div className="wsh-actions">
            <button className="wsh-primary" disabled={locked || !state.readyToCommit} onClick={() => completeMerge(taskId, true)}>
              {op === "commit" ? "コミット中…" : "コミットして push"}
            </button>
            <button disabled={locked || !state.readyToCommit} onClick={() => completeMerge(taskId, false)}>
              コミットのみ
            </button>
          </div>
          {!state.readyToCommit && <span className="tone-warn">まだコミットできる状態ではありません。状態を更新してください。</span>}
        </div>
      )}

      {state?.mergeInProgress && (
        <div className="wsh-actions">
          {!confirmAbort ? (
            <button className="wsh-danger" disabled={locked} onClick={() => setConfirmAbort(true)}>
              取り込みを中止…
            </button>
          ) : (
            <div className="wsh-confirm" style={{ flex: 1 }}>
              <span>取り込みを中止し、merge 前の状態に戻します（解消作業は破棄されます）。</span>
              <div className="wsh-actions">
                <button className="wsh-danger" disabled={locked} onClick={() => abortMerge(taskId)}>
                  {op === "abort" ? "中止中…" : "中止する"}
                </button>
                <button disabled={op === "abort"} onClick={() => setConfirmAbort(false)}>
                  戻る
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
