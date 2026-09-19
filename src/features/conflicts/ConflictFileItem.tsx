/**
 * 競合ファイル 1 件（担当: WS-H）: 内容の確認（作業ツリー / ours / theirs）と解消方法の選択。
 * ours = タスクのブランチ側、theirs = 取り込んだ base 側。
 */
import { useState } from "react";
import type { ConflictFile, ConflictKind, ConflictResolution } from "../../api";
import { usePrStore } from "../../store/prStore";
import { Badge } from "../pr/common";
import type { Label } from "../pr/labels";

const KIND_LABEL: Record<ConflictKind, Label> = {
  bothModified: { text: "両方で変更", tone: "danger" },
  bothAdded: { text: "両方で追加", tone: "danger" },
  deletedByUs: { text: "こちらで削除", tone: "warn" },
  deletedByThem: { text: "base で削除", tone: "warn" },
  other: { text: "その他", tone: "muted" },
};

/** kind ごとの ours / theirs ボタンの文言 */
function choiceLabels(kind: ConflictKind): { ours: string; theirs: string } {
  switch (kind) {
    case "deletedByUs":
      return { ours: "削除を採用", theirs: "base 側を残す" };
    case "deletedByThem":
      return { ours: "こちら側を残す", theirs: "削除を採用" };
    default:
      return { ours: "こちら側（ours）", theirs: "base 側（theirs）" };
  }
}

type View = "working" | "ours" | "theirs";
const VIEWS: { key: View; label: string }[] = [
  { key: "working", label: "作業ツリー" },
  { key: "ours", label: "ours" },
  { key: "theirs", label: "theirs" },
];

const MARKER_RE = /^(<{7}|={7}|>{7}|\|{7})( |$)/m;

export function hasConflictMarkers(text: string | null | undefined): boolean {
  return !!text && MARKER_RE.test(text);
}

function Code({ text }: { text: string | null | undefined }) {
  if (text === undefined) return <p className="muted">読み込み中…</p>;
  if (text === null) return <p className="muted">（この側にはファイルがありません）</p>;
  const lines = text.split("\n");
  return (
    <pre className="cf-code">
      {lines.map((l, i) => (
        <div key={i} className={MARKER_RE.test(l) ? "cf-marker" : undefined}>
          {l || " "}
        </div>
      ))}
    </pre>
  );
}

export function ConflictFileItem({ taskId, file, disabled }: { taskId: string; file: ConflictFile; disabled: boolean }) {
  const content = usePrStore((s) => s.conflictFileByTask[taskId]?.[file.path]);
  const resolving = usePrStore((s) => s.resolvingPathByTask[taskId] === file.path);
  const { loadConflictFile, resolveFile } = usePrStore.getState();

  const [open, setOpen] = useState(false);
  const [view, setView] = useState<View>("working");
  const [markerWarning, setMarkerWarning] = useState(false);

  const labels = choiceLabels(file.kind);

  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (next) void loadConflictFile(taskId, file.path);
  };

  const resolve = (r: ConflictResolution) => void resolveFile(taskId, file.path, r);

  /** 解決済みにする前に、作業ツリーにマーカーが残っていないか確認する */
  const markResolved = async () => {
    if (!markerWarning) {
      await loadConflictFile(taskId, file.path);
      const latest = usePrStore.getState().conflictFileByTask[taskId]?.[file.path];
      if (hasConflictMarkers(latest?.working)) {
        setMarkerWarning(true);
        setOpen(true);
        setView("working");
        return;
      }
    }
    setMarkerWarning(false);
    resolve("markResolved");
  };

  return (
    <li className="cf-file">
      <div className="cf-file-head">
        <button className="wsh-link" onClick={toggle} aria-expanded={open} title="内容を表示">
          {open ? "▾" : "▸"}
        </button>
        <span className="cf-path wsh-mono" title={file.path}>
          {file.path}
        </span>
        <Badge label={KIND_LABEL[file.kind]} />
      </div>

      {open && (
        <>
          <div className="cf-view-tabs">
            {VIEWS.map((v) => (
              <button key={v.key} className={view === v.key ? "active" : undefined} onClick={() => setView(v.key)}>
                {v.label}
              </button>
            ))}
            <span className="wsh-spacer" style={{ flex: 1 }} />
            <button onClick={() => loadConflictFile(taskId, file.path)} title="エディタで編集した内容を読み直す">
              再読込
            </button>
          </div>
          <Code text={content ? content[view] : undefined} />
        </>
      )}

      {markerWarning && (
        <span className="tone-warn">
          作業ツリーにコンフリクトマーカーが残っています。このまま解決済みにしますか？
        </span>
      )}

      <div className="wsh-actions">
        <button disabled={disabled} onClick={() => resolve("ours")} title="タスクのブランチ側の内容を採用">
          {labels.ours}
        </button>
        <button disabled={disabled} onClick={() => resolve("theirs")} title="base 側の内容を採用">
          {labels.theirs}
        </button>
        <button
          disabled={disabled}
          onClick={markResolved}
          title="エディタや AI で編集した作業ツリーの内容をそのまま採用（git add）"
        >
          {markerWarning ? "それでも解決済みにする" : "編集済み → 解決済みにする"}
        </button>
        {markerWarning && (
          <button disabled={disabled} onClick={() => setMarkerWarning(false)}>
            やめる
          </button>
        )}
        {resolving && <span className="muted">反映中…</span>}
      </div>
    </li>
  );
}
