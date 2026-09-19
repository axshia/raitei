/**
 * タスクタブストア（担当: WS-F）。開いているタスクタブと、タブ内のサブビュー選択を管理する。
 *
 * - 開いているタブ・アクティブタブ・サブビュー選択は localStorage に保存し、再起動後に復元する
 * - 存在しないタスクのタブは App 側で `pruneTabs()` により取り除く
 * - 他ストアから画面を切り替えたいときは `useTabStore.getState().setSubView(taskId, "conflicts")` のように呼ぶ
 */
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

/** タスクタブ内のサブビュー */
export type TaskSubView = "chat" | "pr" | "conflicts" | "changes";

/** @deprecated 旧名。`TaskSubView` を使う */
export type SidePanelKind = TaskSubView;

export const DEFAULT_SUB_VIEW: TaskSubView = "chat";

interface TabState {
  openTaskIds: string[];
  activeTaskId: string | null;
  subViewByTask: Record<string, TaskSubView>;
  /** サイドバーで折りたたんだプロジェクト */
  collapsedProjects: Record<string, boolean>;

  /** タブを開いてアクティブにする。`subView` 指定時はそのサブビューに切り替える */
  openTask(taskId: string, subView?: TaskSubView): void;
  closeTask(taskId: string): void;
  activateTask(taskId: string): void;
  /** 相対移動（-1: 左, +1: 右）。端では循環する */
  cycleTab(delta: number): void;
  setSubView(taskId: string, view: TaskSubView): void;
  /** @deprecated `setSubView` を使う */
  setSidePanel(taskId: string, view: TaskSubView): void;
  /** 存在するタスク ID 以外のタブを閉じる */
  pruneTabs(existingTaskIds: string[]): void;
  toggleProjectCollapsed(projectId: string): void;
}

export const useTabStore = create<TabState>()(
  persist(
    (set, get) => ({
      openTaskIds: [],
      activeTaskId: null,
      subViewByTask: {},
      collapsedProjects: {},

      openTask: (taskId, subView) =>
        set((s) => ({
          openTaskIds: s.openTaskIds.includes(taskId) ? s.openTaskIds : [...s.openTaskIds, taskId],
          activeTaskId: taskId,
          subViewByTask: subView ? { ...s.subViewByTask, [taskId]: subView } : s.subViewByTask,
        })),
      closeTask: (taskId) =>
        set((s) => {
          const idx = s.openTaskIds.indexOf(taskId);
          if (idx < 0) return s;
          const openTaskIds = s.openTaskIds.filter((id) => id !== taskId);
          const activeTaskId =
            s.activeTaskId === taskId ? (openTaskIds[Math.min(idx, openTaskIds.length - 1)] ?? null) : s.activeTaskId;
          const { [taskId]: _dropped, ...subViewByTask } = s.subViewByTask;
          return { openTaskIds, activeTaskId, subViewByTask };
        }),
      activateTask: (taskId) => set({ activeTaskId: taskId }),
      cycleTab: (delta) => {
        const { openTaskIds, activeTaskId } = get();
        if (openTaskIds.length === 0) return;
        const idx = activeTaskId ? openTaskIds.indexOf(activeTaskId) : -1;
        const next = (idx + delta + openTaskIds.length) % openTaskIds.length;
        set({ activeTaskId: openTaskIds[next] });
      },
      setSubView: (taskId, view) => set((s) => ({ subViewByTask: { ...s.subViewByTask, [taskId]: view } })),
      setSidePanel: (taskId, view) => get().setSubView(taskId, view),
      pruneTabs: (existing) =>
        set((s) => {
          const keep = new Set(existing);
          const openTaskIds = s.openTaskIds.filter((id) => keep.has(id));
          if (openTaskIds.length === s.openTaskIds.length) return s;
          const activeTaskId =
            s.activeTaskId && keep.has(s.activeTaskId) ? s.activeTaskId : (openTaskIds[openTaskIds.length - 1] ?? null);
          return { openTaskIds, activeTaskId };
        }),
      toggleProjectCollapsed: (projectId) =>
        set((s) => ({ collapsedProjects: { ...s.collapsedProjects, [projectId]: !s.collapsedProjects[projectId] } })),
    }),
    {
      name: "raitei.tabs",
      version: 1,
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({
        openTaskIds: s.openTaskIds,
        activeTaskId: s.activeTaskId,
        subViewByTask: s.subViewByTask,
        collapsedProjects: s.collapsedProjects,
      }),
    },
  ),
);

/**
 * そのタスクのサブビューが現在画面に表示されているか。
 * サブビューは一度開くと非表示でもマウントされたままなので、ポーリング等は表示中だけ行うこと。
 */
export const useIsSubViewVisible = (taskId: string, view: TaskSubView) =>
  useTabStore((s) => s.activeTaskId === taskId && (s.subViewByTask[taskId] ?? DEFAULT_SUB_VIEW) === view);
