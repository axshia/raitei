/**
 * PR マージ操作（担当: WS-H）: 方式選択（merge / squash / rebase）・リモートブランチ削除・確認付き実行。
 * マージを止める条件（クローズ・ドラフト・コンフリクト）はボタンを無効化し、
 * CI 未完了・失敗やレビュー未承認は確認ステップで警告してから実行できるようにする。
 */
import { useState } from "react";
import type { MergeMethod, PullRequestStatus } from "../../api";
import { usePrStore } from "../../store/prStore";

const METHODS: { key: MergeMethod; label: string; hint: string }[] = [
  { key: "merge", label: "Merge commit", hint: "マージコミットを作成します" },
  { key: "squash", label: "Squash and merge", hint: "1 つのコミットにまとめて取り込みます" },
  { key: "rebase", label: "Rebase and merge", hint: "コミットを base の先頭に並べ直して取り込みます" },
];

const METHOD_KEY = "raitei.prMergeMethod";

function loadMethod(): MergeMethod {
  const v = localStorage.getItem(METHOD_KEY);
  return v === "merge" || v === "rebase" || v === "squash" ? v : "squash";
}

/** マージできない理由（あればボタンを無効化） */
function blockingReason(pr: PullRequestStatus): string | null {
  if (pr.state === "merged") return "マージ済みです";
  if (pr.state === "closed") return "PR はクローズされています";
  if (pr.isDraft) return "ドラフトの PR はマージできません。GitHub で Ready for review にしてください";
  if (pr.hasConflicts) return "コンフリクトを解消するまでマージできません";
  return null;
}

/** 確認時に出す警告 */
function warnings(pr: PullRequestStatus): string[] {
  const w: string[] = [];
  if (pr.checks.failed > 0) w.push(`CI チェックが ${pr.checks.failed} 件失敗しています`);
  if (pr.checks.pending > 0) w.push(`CI チェックが ${pr.checks.pending} 件実行中です`);
  if (pr.reviewDecision === "CHANGES_REQUESTED") w.push("変更要求のレビューがあります");
  if (pr.reviewDecision === "REVIEW_REQUIRED") w.push("必要なレビューの承認がまだありません");
  if (pr.mergeStateStatus.toUpperCase() === "BLOCKED") w.push("保護ルールでブロックされているため、失敗する可能性があります");
  if (pr.mergeStateStatus.toUpperCase() === "BEHIND") w.push("base より遅れています");
  if (pr.mergeable === "unknown") w.push("GitHub がまだマージ可否を判定中です");
  return w;
}

export function PrMergeControls({ taskId, pr }: { taskId: string; pr: PullRequestStatus }) {
  const merging = usePrStore((s) => s.prOpByTask[taskId] === "merge");
  const busy = usePrStore((s) => !!s.prOpByTask[taskId]);
  const mergePr = usePrStore((s) => s.mergePr);

  const [method, setMethod] = useState<MergeMethod>(loadMethod);
  const [deleteRemoteBranch, setDeleteRemoteBranch] = useState(true);
  const [confirming, setConfirming] = useState(false);

  if (pr.state !== "open") return null;

  const reason = blockingReason(pr);
  const warns = warnings(pr);
  const current = METHODS.find((m) => m.key === method)!;

  const selectMethod = (m: MergeMethod) => {
    setMethod(m);
    localStorage.setItem(METHOD_KEY, m);
  };

  const doMerge = async () => {
    const ok = await mergePr({ taskId, method, deleteRemoteBranch });
    if (ok) setConfirming(false);
  };

  return (
    <div className="wsh-section">
      <div className="wsh-section-title">マージ</div>
      <label className="wsh-field">
        <span>方式</span>
        <select value={method} onChange={(e) => selectMethod(e.target.value as MergeMethod)} disabled={merging}>
          {METHODS.map((m) => (
            <option key={m.key} value={m.key}>
              {m.label}
            </option>
          ))}
        </select>
      </label>
      <span className="muted" style={{ fontSize: 12 }}>
        {current.hint}
      </span>
      <label className="wsh-check">
        <input
          type="checkbox"
          checked={deleteRemoteBranch}
          onChange={(e) => setDeleteRemoteBranch(e.target.checked)}
          disabled={merging}
        />
        マージ後にリモートブランチを削除
      </label>
      <span className="muted" style={{ fontSize: 12 }}>
        ローカルのブランチと worktree は残ります（タスク削除時に片付けます）。
      </span>

      {reason && <span className="tone-warn">{reason}</span>}

      {!confirming ? (
        <div className="wsh-actions">
          <button className="wsh-primary" disabled={!!reason || busy} onClick={() => setConfirming(true)}>
            {current.label}
          </button>
        </div>
      ) : (
        <div className="wsh-confirm">
          <strong>
            #{pr.number} を「{current.label}」でマージしますか？
          </strong>
          {warns.length > 0 && (
            <ul className="tone-warn">
              {warns.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          )}
          <div className="wsh-actions">
            <button className="wsh-primary" disabled={!!reason || busy} onClick={doMerge}>
              {merging ? "マージ中…" : warns.length > 0 ? "警告を承知でマージ" : "マージする"}
            </button>
            <button disabled={merging} onClick={() => setConfirming(false)}>
              キャンセル
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
