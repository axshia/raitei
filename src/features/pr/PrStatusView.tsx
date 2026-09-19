/**
 * PR 状態表示（担当: WS-H）: 概要・CI チェック・レビュー・mergeable / コンフリクト。
 */
import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ChecksSummary, PullRequestStatus, Review } from "../../api";
import { useTabStore } from "../../store/tabStore";
import { Badge } from "./common";
import {
  checkIcon,
  checkStatusLabel,
  mergeStateLabel,
  reviewDecisionLabel,
  reviewStateLabel,
  type Label,
} from "./labels";

function open(url: string | null) {
  if (url) void openUrl(url).catch(() => undefined);
}

function prStateLabel(pr: PullRequestStatus): Label {
  if (pr.state === "merged") return { text: "マージ済み", tone: "accent" };
  if (pr.state === "closed") return { text: "クローズ", tone: "danger" };
  if (pr.isDraft) return { text: "ドラフト", tone: "muted" };
  return { text: "オープン", tone: "ok" };
}

export function PrHeader({ pr }: { pr: PullRequestStatus }) {
  return (
    <div className="wsh-section">
      <div className="wsh-section-title">
        <Badge label={prStateLabel(pr)} />
        <span className="wsh-spacer" />
        <button className="wsh-link" onClick={() => open(pr.url)} title={pr.url}>
          GitHub で開く ↗
        </button>
      </div>
      <div style={{ fontWeight: 600, wordBreak: "break-word" }}>
        <span className="muted">#{pr.number}</span> {pr.title}
      </div>
      <div className="muted wsh-mono">
        {pr.headBranch} → {pr.baseBranch}
      </div>
    </div>
  );
}

function checksOverall(c: ChecksSummary): Label {
  if (c.total === 0) return { text: "チェックなし", tone: "muted" };
  if (c.failed > 0) return { text: `${c.failed} 件失敗`, tone: "danger" };
  if (c.pending > 0) return { text: `${c.pending} 件実行中`, tone: "warn" };
  return { text: "すべて成功", tone: "ok" };
}

/** 失敗 → 実行中 → その他 → 成功 の順に並べる */
const CHECK_ORDER: Record<string, number> = { failure: 0, cancelled: 1, pending: 2, unknown: 3, neutral: 4, skipped: 5, success: 6 };

export function PrChecks({ checks }: { checks: ChecksSummary }) {
  const [expanded, setExpanded] = useState(false);
  const overall = checksOverall(checks);
  const sorted = [...checks.checks].sort((a, b) => (CHECK_ORDER[a.status] ?? 3) - (CHECK_ORDER[b.status] ?? 3));
  const others = checks.total - checks.passed - checks.failed - checks.pending;
  // 失敗・実行中があれば最初から展開する
  const show = expanded || checks.failed > 0 || checks.pending > 0;

  return (
    <div className="wsh-section">
      <div className="wsh-section-title">
        CI チェック
        <Badge label={overall} />
        <span className="wsh-spacer" />
        {checks.total > 0 && (
          <span className="muted">
            {checks.passed}/{checks.total} 成功
          </span>
        )}
      </div>
      {checks.total > 0 && (
        <div className="wsh-progress" aria-hidden>
          <span style={{ width: `${(checks.passed / checks.total) * 100}%`, background: "var(--ok)" }} />
          <span style={{ width: `${(checks.failed / checks.total) * 100}%`, background: "var(--danger)" }} />
          <span style={{ width: `${(checks.pending / checks.total) * 100}%`, background: "var(--warn)" }} />
          <span style={{ width: `${(others / checks.total) * 100}%`, background: "var(--fg-muted)" }} />
        </div>
      )}
      {checks.total > 0 && !show && (
        <button className="wsh-link" onClick={() => setExpanded(true)}>
          {checks.total} 件の詳細を表示
        </button>
      )}
      {show && (
        <ul className="wsh-list">
          {sorted.map((c, i) => {
            const label = checkStatusLabel(c.status);
            return (
              <li key={`${c.name}-${i}`}>
                <span className={`tone-${label.tone}`} title={label.text} style={{ width: 14, textAlign: "center" }}>
                  {checkIcon(c.status)}
                </span>
                {c.url ? (
                  <button className="wsh-link wsh-name" onClick={() => open(c.url)} title={c.url}>
                    {c.name}
                  </button>
                ) : (
                  <span className="wsh-name">{c.name}</span>
                )}
                <span className={`tone-${label.tone}`} style={{ fontSize: 11 }}>
                  {label.text}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

/** レビュアーごとに最新のレビューだけ残す */
function latestByAuthor(reviews: Review[]): Review[] {
  const map = new Map<string, Review>();
  for (const r of reviews) {
    const prev = map.get(r.author);
    if (!prev || (r.submittedAt ?? "") >= (prev.submittedAt ?? "")) map.set(r.author, r);
  }
  return [...map.values()];
}

export function PrReviews({ pr }: { pr: PullRequestStatus }) {
  const reviews = latestByAuthor(pr.reviews);
  return (
    <div className="wsh-section">
      <div className="wsh-section-title">
        レビュー
        <Badge label={reviewDecisionLabel(pr.reviewDecision)} />
      </div>
      {reviews.length === 0 ? (
        <span className="muted">レビューはまだありません</span>
      ) : (
        <ul className="wsh-list">
          {reviews.map((r) => (
            <li key={r.author}>
              <span className="wsh-name">@{r.author}</span>
              <Badge label={reviewStateLabel(r.state)} />
              {r.submittedAt && (
                <span className="muted" style={{ fontSize: 11 }}>
                  {new Date(r.submittedAt).toLocaleString()}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function PrMergeability({ pr, taskId }: { pr: PullRequestStatus; taskId: string }) {
  const mergeable: Label =
    pr.mergeable === "mergeable"
      ? { text: "可", tone: "ok" }
      : pr.mergeable === "conflicting"
        ? { text: "不可（コンフリクト）", tone: "danger" }
        : { text: "判定中", tone: "muted" };
  const mergeState = mergeStateLabel(pr.mergeStateStatus);
  const needsBaseMerge = pr.hasConflicts || pr.mergeStateStatus.toUpperCase() === "BEHIND";

  return (
    <div className="wsh-section">
      <div className="wsh-section-title">マージ可否</div>
      <dl className="wsh-kv" style={{ margin: 0 }}>
        <dt>mergeable</dt>
        <dd>
          <Badge label={mergeable} />
        </dd>
        <dt>状態</dt>
        <dd className={`tone-${mergeState.tone}`}>
          {mergeState.text} <span className="muted wsh-mono">({pr.mergeStateStatus || "UNKNOWN"})</span>
        </dd>
        <dt>コンフリクト</dt>
        <dd>
          <Badge label={pr.hasConflicts ? { text: "あり", tone: "danger" } : { text: "なし", tone: "ok" }} />
        </dd>
      </dl>
      {pr.state === "open" && needsBaseMerge && (
        <div className="wsh-actions">
          <button onClick={() => useTabStore.getState().setSidePanel(taskId, "conflicts")}>
            {pr.hasConflicts ? "コンフリクトを解消する →" : "base を取り込む →"}
          </button>
        </div>
      )}
    </div>
  );
}
