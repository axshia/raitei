/**
 * サイドバー内の 1 プロジェクト（担当: WS-F）:
 * 見出し（折りたたみ・タスク数・新規タスク・メニュー）+ タスク一覧。
 * メニューから worktree 一覧、Finder 表示、登録解除を行う。
 */
import { useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { Project } from "../../api";
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";
import { Menu } from "../../components/layout/Menu";
import { ConfirmDialog } from "../../components/layout/ConfirmDialog";
import { TaskList } from "../tasks/TaskList";
import { NewTaskDialog } from "../tasks/NewTaskForm";
import { WorktreeDialog } from "../worktrees/WorktreePanel";
import { shortenHome } from "../tasks/branch";

type Dialog = "newTask" | "worktrees" | "remove" | null;

export function ProjectSection({ project }: { project: Project }) {
  const taskCount = useProjectStore((s) => s.tasksByProject[project.id]?.length ?? 0);
  const selected = useProjectStore((s) => s.selectedProjectId === project.id);
  const selectProject = useProjectStore((s) => s.selectProject);
  const loadTasks = useProjectStore((s) => s.loadTasks);
  const removeProject = useProjectStore((s) => s.removeProject);
  const collapsed = useTabStore((s) => !!s.collapsedProjects[project.id]);
  const toggle = useTabStore((s) => s.toggleProjectCollapsed);
  const [dialog, setDialog] = useState<Dialog>(null);

  return (
    <section className={`project ${selected ? "selected" : ""}`}>
      <div
        className="project-header"
        title={`${project.repoPath}\n既定ブランチ: ${project.defaultBranch}`}
        onClick={() => {
          selectProject(project.id);
          toggle(project.id);
        }}
      >
        <span className={`caret ${collapsed ? "" : "open"}`} />
        <span className="project-name truncate">{project.name}</span>
        <span className="project-count subtle">{taskCount || ""}</span>
        <span className="project-actions" onClick={(e) => e.stopPropagation()}>
          <button className="ghost icon" title="新しいタスク" onClick={() => setDialog("newTask")}>
            +
          </button>
          <Menu
            label="プロジェクトのメニュー"
            trigger={() => (
              <button className="ghost icon" title="メニュー">
                ⋯
              </button>
            )}
            items={[
              { label: "新しいタスク…", onSelect: () => setDialog("newTask") },
              { label: "Worktree 一覧…", onSelect: () => setDialog("worktrees") },
              "separator",
              { label: "Finder で表示", onSelect: () => void revealItemInDir(project.repoPath).catch(() => {}) },
              { label: "タスク一覧を再読み込み", onSelect: () => void loadTasks(project.id).catch(() => {}) },
              "separator",
              { label: "登録を解除…", danger: true, onSelect: () => setDialog("remove") },
            ]}
          />
        </span>
      </div>

      {!collapsed && (
        <>
          <div className="project-path subtle mono truncate">{shortenHome(project.repoPath)}</div>
          <TaskList projectId={project.id} onNewTask={() => setDialog("newTask")} />
        </>
      )}

      {dialog === "newTask" && <NewTaskDialog project={project} onClose={() => setDialog(null)} />}
      {dialog === "worktrees" && <WorktreeDialog project={project} onClose={() => setDialog(null)} />}
      {dialog === "remove" && (
        <ConfirmDialog
          title={`「${project.name}」の登録を解除`}
          confirmLabel="登録を解除"
          danger
          onClose={() => setDialog(null)}
          onConfirm={async () => {
            try {
              await removeProject(project.id);
            } finally {
              // エラーはダイアログ内に出すので、サイドバー側の表示は消す
              useProjectStore.getState().clearError();
            }
          }}
        >
          <p style={{ margin: 0 }}>
            raitei の一覧から外します。リポジトリ・worktree・ブランチのファイルは削除しません。
          </p>
          {taskCount > 0 && <p className="muted" style={{ margin: 0 }}>このプロジェクトのタスク {taskCount} 件のタブは閉じます。</p>}
        </ConfirmDialog>
      )}
    </section>
  );
}
