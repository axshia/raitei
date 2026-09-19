/**
 * タスクタブストア（担当: WS-F）。開いているタスクタブと、タブ内の右パネル選択を管理する。
 */
import { create } from "zustand";

/** タスクタブ右側のパネル */
export type SidePanelKind = "pr" | "conflicts" | "worktree";

interface TabState {
  openTaskIds: string[];
  activeTaskId: string | null;
  sidePanelByTask: Record<string, SidePanelKind>;

  openTask(taskId: string): void;
  closeTask(taskId: string): void;
  activateTask(taskId: string): void;
  setSidePanel(taskId: string, panel: SidePanelKind): void;
}

export const useTabStore = create<TabState>((set) => ({
  openTaskIds: [],
  activeTaskId: null,
  sidePanelByTask: {},

  openTask: (taskId) =>
    set((s) => ({
      openTaskIds: s.openTaskIds.includes(taskId) ? s.openTaskIds : [...s.openTaskIds, taskId],
      activeTaskId: taskId,
    })),
  closeTask: (taskId) =>
    set((s) => {
      const idx = s.openTaskIds.indexOf(taskId);
      const openTaskIds = s.openTaskIds.filter((id) => id !== taskId);
      const activeTaskId =
        s.activeTaskId === taskId ? (openTaskIds[Math.min(idx, openTaskIds.length - 1)] ?? null) : s.activeTaskId;
      return { openTaskIds, activeTaskId };
    }),
  activateTask: (taskId) => set({ activeTaskId: taskId }),
  setSidePanel: (taskId, panel) => set((s) => ({ sidePanelByTask: { ...s.sidePanelByTask, [taskId]: panel } })),
}));
