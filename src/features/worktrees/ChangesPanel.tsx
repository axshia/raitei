/**
 * 「変更」サブビュー（担当: WS-F）: タスク worktree の git status（ブランチ・upstream・ahead/behind・変更ファイル）。
 * 表示中になったとき・エージェントの run 完了時・手動で再取得する。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { gitApi, toAppError, type AppError, type FileChange, type GitStatus } from "../../api";
import { useTabStore, useIsSubViewVisible } from "../../store/tabStore";
import { useAgentStore } from "../../store/agentStore";
import "./worktrees.css";

const CONFLICT_CODES = new Set(["UU", "AA", "DD", "AU", "UA", "DU", "UD"]);

type Group = "conflicted" | "staged" | "unstaged" | "untracked";
const GROUPS: { key: Group; label: string }[] = [
  { key: "conflicted", label: "競合" },
  { key: "staged", label: "ステージ済み" },
  { key: "unstaged", label: "未ステージ" },
  { key: "untracked", label: "未追跡" },
];

const isChanged = (c: string | undefined) => !!c && c !== " " && c !== ".";

function groupFiles(files: FileChange[]): Record<Group, { file: FileChange; code: string }[]> {
  const out: Record<Group, { file: FileChange; code: string }[]> = { conflicted: [], staged: [], unstaged: [], untracked: [] };
  for (const f of files) {
    const xy = f.status.padEnd(2, " ");
    if (xy === "??") out.untracked.push({ file: f, code: "?" });
    else if (CONFLICT_CODES.has(xy)) out.conflicted.push({ file: f, code: xy });
    else {
      if (isChanged(xy[0])) out.staged.push({ file: f, code: xy[0] });
      if (isChanged(xy[1])) out.unstaged.push({ file: f, code: xy[1] });
      if (!isChanged(xy[0]) && !isChanged(xy[1])) out.unstaged.push({ file: f, code: xy.trim() || "?" });
    }
  }
  return out;
}

const CODE_LABEL: Record<string, string> = {
  M: "変更",
  A: "追加",
  D: "削除",
  R: "名前変更",
  C: "コピー",
  T: "種別変更",
  "?": "未追跡",
};

function splitPath(path: string) {
  const i = path.lastIndexOf("/");
  return i < 0 ? { dir: "", name: path } : { dir: path.slice(0, i + 1), name: path.slice(i + 1) };
}

export function ChangesPanel({ taskId }: { taskId: string }) {
  const visible = useIsSubViewVisible(taskId, "changes");
  const running = useAgentStore((s) => !!s.runningByTask[taskId]);
  const setSubView = useTabStore((s) => s.setSubView);
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(false);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setStatus(await gitApi.getGitStatus(taskId));
      setError(null);
      setUpdatedAt(new Date());
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setLoading(false);
    }
  }, [taskId]);

  // 表示されたときに取得
  useEffect(() => {
    if (visible) void refresh();
  }, [visible, refresh]);

  // run 完了（running: true → false）で取得
  const wasRunning = useRef(running);
  useEffect(() => {
    if (wasRunning.current && !running && visible) void refresh();
    wasRunning.current = running;
  }, [running, visible, refresh]);

  const groups = status ? groupFiles(status.files) : null;

  return (
    <div className="changes">
      <div className="changes-toolbar toolbar">
        {status ? (
          <>
            <span className="mono">{status.branch ?? "(detached)"}</span>
            {status.upstream ? (
              <span className="mono subtle">→ {status.upstream}</span>
            ) : (
              <span className="badge" title="まだ push されていません">upstream なし</span>
            )}
            {status.ahead > 0 && <span className="badge accent" title="未 push のコミット">↑{status.ahead}</span>}
            {status.behind > 0 && <span className="badge warn" title="upstream に遅れているコミット">↓{status.behind}</span>}
            <span className="muted">{status.files.length} ファイル</span>
          </>
        ) : (
          <span className="muted">git status</span>
        )}
        <span className="spacer" />
        {running && (
          <span className="muted toolbar">
            <span className="dot running" /> エージェント実行中
          </span>
        )}
        {updatedAt && <span className="subtle">{updatedAt.toLocaleTimeString()}</span>}
        <button className="sm" onClick={() => void refresh()} disabled={loading}>
          {loading ? <span className="spinner" /> : "↻"} 更新
        </button>
      </div>

      <div className="panel-scroll">
        {error && (
          <div className="banner danger changes-banner">
            <span className="grow selectable">
              <b>{error.kind}</b> {error.message}
            </span>
          </div>
        )}
        {status?.mergeInProgress && (
          <div className="banner warn changes-banner">
            <span className="grow">base の取り込み（マージ）が進行中です。</span>
            <button className="sm" onClick={() => setSubView(taskId, "conflicts")}>
              コンフリクトを開く
            </button>
          </div>
        )}
        {!status && !error && (
          <div className="empty">
            <span className="spinner" />
          </div>
        )}
        {status && status.files.length === 0 && (
          <div className="empty">
            <div>作業ツリーに変更はありません</div>
            {status.ahead > 0 && <div className="subtle">未 push のコミットが {status.ahead} 件あります</div>}
          </div>
        )}
        {groups &&
          GROUPS.filter((g) => groups[g.key].length > 0).map((g) => (
            <section key={g.key} className="changes-group">
              <div className="changes-group-title section-title">
                {g.label} <span className="subtle">{groups[g.key].length}</span>
              </div>
              <ul className="file-list">
                {groups[g.key].map(({ file, code }) => {
                  const { dir, name } = splitPath(file.path);
                  return (
                    <li key={`${g.key}:${file.path}`} className="file-row" title={`${file.status}  ${file.path}`}>
                      <span className={`file-code code-${g.key === "conflicted" ? "U" : code}`} title={CODE_LABEL[code] ?? code}>
                        {code}
                      </span>
                      <span className="file-path mono selectable truncate">
                        <span>{name}</span>
                        {dir && <span className="subtle"> {dir}</span>}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </section>
          ))}
      </div>
    </div>
  );
}
