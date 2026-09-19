/** gh の列挙値を画面表示用の日本語ラベルと色に変換する（PR / コンフリクト共通） */

export type Tone = "ok" | "warn" | "danger" | "muted" | "accent";

export interface Label {
  text: string;
  tone: Tone;
}

/** CI チェックの正規化済み status（github/parse.rs の出力） */
export function checkStatusLabel(status: string): Label {
  switch (status) {
    case "success":
      return { text: "成功", tone: "ok" };
    case "failure":
      return { text: "失敗", tone: "danger" };
    case "cancelled":
      return { text: "キャンセル", tone: "danger" };
    case "pending":
      return { text: "実行中", tone: "warn" };
    case "neutral":
      return { text: "中立", tone: "muted" };
    case "skipped":
      return { text: "スキップ", tone: "muted" };
    default:
      return { text: "不明", tone: "muted" };
  }
}

export function checkIcon(status: string): string {
  switch (status) {
    case "success":
      return "✓";
    case "failure":
    case "cancelled":
      return "✕";
    case "pending":
      return "●";
    default:
      return "–";
  }
}

/** PR 全体の reviewDecision */
export function reviewDecisionLabel(decision: string | null): Label {
  switch (decision) {
    case "APPROVED":
      return { text: "承認済み", tone: "ok" };
    case "CHANGES_REQUESTED":
      return { text: "変更要求あり", tone: "danger" };
    case "REVIEW_REQUIRED":
      return { text: "レビュー待ち", tone: "warn" };
    default:
      return { text: "レビュー要件なし", tone: "muted" };
  }
}

/** 個々のレビューの state */
export function reviewStateLabel(state: string): Label {
  switch (state) {
    case "APPROVED":
      return { text: "承認", tone: "ok" };
    case "CHANGES_REQUESTED":
      return { text: "変更要求", tone: "danger" };
    case "COMMENTED":
      return { text: "コメント", tone: "muted" };
    case "DISMISSED":
      return { text: "取り下げ", tone: "muted" };
    case "PENDING":
      return { text: "下書き", tone: "muted" };
    default:
      return { text: state, tone: "muted" };
  }
}

/** mergeStateStatus の説明（GitHub GraphQL の MergeStateStatus） */
export function mergeStateLabel(status: string): Label {
  switch (status.toUpperCase()) {
    case "CLEAN":
      return { text: "マージできます", tone: "ok" };
    case "HAS_HOOKS":
      return { text: "マージできます（フックあり）", tone: "ok" };
    case "UNSTABLE":
      return { text: "必須ではないチェックが失敗しています", tone: "warn" };
    case "BLOCKED":
      return { text: "保護ルール（必須チェック・レビュー）でブロック中", tone: "danger" };
    case "BEHIND":
      return { text: "base より遅れています（取り込みが必要）", tone: "warn" };
    case "DIRTY":
      return { text: "コンフリクトがあります", tone: "danger" };
    case "DRAFT":
      return { text: "ドラフトのためマージできません", tone: "muted" };
    default:
      return { text: "GitHub が判定中です", tone: "muted" };
  }
}
