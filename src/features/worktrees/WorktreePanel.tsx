/**
 * Worktree 一覧（担当: WS-F）: プロジェクトの `git worktree list` を表示し、タスクに紐づく worktree を削除できる。
 * タスクに紐づかない worktree（raitei 外で作ったもの）は、削除 API が無いため表示のみ。
 */
import { useCallback, useEffect, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { gitApi, toAppError, type AppError, type Project, type Task, type WorktreeInfo } from "../../api";
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";
import { Modal } from "../../components/layout/Modal";
import { DeleteTaskDialog } from "../tasks/DeleteTaskDialog";
import { shortenHome } from "../tasks/branch";
import "./worktrees.css";

const NO_TASKS: Task[] = [];

function Flags({ wt }: { wt: WorktreeInfo }) {
  return (
    <>
      {wt.isMain && <span className="badge accent">main</span>}
      {wt.isBare && <span className="badge">bare</span>}
      {wt.isDetached && <span className="badge warn">detached</span>}
      {wt.locked && <span className="badge warn">locked</span>}
      {wt.prunable && <span className="badge danger" title="ディレクトリが存在しません（git worktree prune 対象）">prunable</span>}
    </>
  );
}

export function WorktreeList({ project, onOpenTask }: { project: Project; onOpenTask?(): void }) {
  const tasks = useProjectStore((s) => s.tasksByProject[project.id] ?? NO_TASKS);
  const openTask = useTabStore((s) => s.openTask);
  const [items, setItems] = useState<WorktreeInfo[] | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loading, setLoading] = useState(false);
  const [deleting, setDeleting] = useState<Task | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setItems(await gitApi.listWorktrees(project.id));
      setError(null);
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setLoading(false);
    }
  }, [project.id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // taskId が埋まっていない場合に備えてパスでも突き合わせる
  const taskFor = (wt: WorktreeInfo) =>
    tasks.find((t) => t.id === wt.taskId) ?? tasks.find((t) => t.worktreePath.replace(/\/+$/, "") === wt.path.replace(/\/+$/, ""));

  return (
    <div className="worktree-list">
      <div className="toolbar">
        <span className="muted">{items ? `${items.length} 件` : ""}</span>
        <span className="spacer" />
        <button type="button" className="sm" onClick={() => void refresh()} disabled={loading}>
          {loading ? <span className="spinner" /> : "↻"} 更新
        </button>
      </div>
      {error && (
        <div className="banner danger">
          <span className="grow selectable">{error.message}</span>
        </div>
      )}
      {items && (
        <table className="wt-table">
          <thead>
            <tr>
              <th>ブランチ / パス</th>
              <th>HEAD</th>
              <th>タスク</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {items.map((wt) => {
              const task = taskFor(wt);
              return (
                <tr key={wt.path}>
                  <td>
                    <div className="wt-main">
                    <div className="toolbar">
                      <span className="mono truncate">{wt.branch ?? "(detached)"}</span>
                      <Flags wt={wt} />
                    </div>
                    <div className="mono subtle truncate selectable" title={wt.path}>
                      {shortenHome(wt.path)}
                    </div>
                    </div>
                  </td>
                  <td className="mono muted">{wt.head?.slice(0, 7) ?? "—"}</td>
                  <td className="wt-task">
                    {task ? (
                      <button
                        type="button"
                        className="ghost sm"
                        title="タブで開く"
                        onClick={() => {
                          openTask(task.id);
                          onOpenTask?.();
                        }}
                      >
                        <span className={`agent-mark ${task.agent}`}>{task.agent === "claude" ? "CL" : "CX"}</span>
                        <span className="truncate">{task.title}</span>
                      </button>
                    ) : (
                      <span className="subtle">{wt.isMain ? "本体" : "raitei 管理外"}</span>
                    )}
                  </td>
                  <td className="wt-actions">
                    <button
                      type="button"
                      className="ghost icon"
                      title="Finder で表示"
                      disabled={wt.prunable}
                      onClick={() => void revealItemInDir(wt.path).catch(() => {})}
                    >
                      ↗
                    </button>
                    <button
                      type="button"
                      className="ghost sm danger-text"
                      title={task ? "タスクと worktree を削除…" : "raitei 管理外の worktree は削除できません"}
                      disabled={!task}
                      onClick={() => task && setDeleting(task)}
                    >
                      削除
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
      {deleting && (
        <DeleteTaskDialog
          task={deleting}
          onClose={() => {
            setDeleting(null);
            void refresh();
          }}
        />
      )}
    </div>
  );
}

export function WorktreeDialog({ project, onClose }: { project: Project; onClose(): void }) {
  return (
    <Modal title={`Worktree — ${project.name}`} onClose={onClose} width={720}>
      <WorktreeList project={project} onOpenTask={onClose} />
    </Modal>
  );
}
