/** PR / コンフリクトパネルで共用する小さな表示部品（担当: WS-H） */
import type { AppError } from "../../api";
import type { Notice } from "../../store/prStore";
import type { Label } from "./labels";
import "./pr.css";

export function Badge({ label, icon }: { label: Label; icon?: string }) {
  return (
    <span className={`wsh-badge tone-${label.tone}`}>
      {icon && <span aria-hidden>{icon}</span>}
      {label.text}
    </span>
  );
}

const ERROR_KIND_TEXT: Partial<Record<AppError["kind"], string>> = {
  notImplemented: "未実装",
  gh: "gh",
  git: "git",
  invalidInput: "入力エラー",
  notFound: "見つかりません",
};

export function ErrorNotice({ error, onClose }: { error: AppError | null | undefined; onClose?: () => void }) {
  if (!error) return null;
  const kind = ERROR_KIND_TEXT[error.kind];
  return (
    <div className="wsh-notice error" role="alert">
      <span>
        {kind && <strong>[{kind}] </strong>}
        {error.message}
      </span>
      {onClose && (
        <button aria-label="閉じる" onClick={onClose}>
          ×
        </button>
      )}
    </div>
  );
}

export function NoticeView({ notice }: { notice: Notice | null | undefined }) {
  if (!notice) return null;
  return <div className={`wsh-notice ${notice.kind}`}>{notice.text}</div>;
}

/** 経過時間の短い表記（例: 「12 秒前」） */
export function formatAgo(ms: number | undefined, now: number): string {
  if (!ms) return "未取得";
  const sec = Math.max(0, Math.round((now - ms) / 1000));
  if (sec < 60) return `${sec} 秒前`;
  const min = Math.round(sec / 60);
  if (min < 60) return `${min} 分前`;
  return `${Math.round(min / 60)} 時間前`;
}
