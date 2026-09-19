/**
 * プロジェクト・タスク一覧ストア（担当: WS-F）。
 * 公開する state / action 名は契約。内部実装は自由に変更してよい。
 *
 * 追加分（契約に対して追加のみ・既存名は不変）:
 * - `loaded`: 初回 `loadProjects` が完了したか（タブ復元時の存在チェックに使う）
 * - `environment` / `loadEnvironment`: git / gh / claude / codex の検出結果
 * - `updateTask`: タスクのタイトル・エージェント・権限の変更
 *
 * タスク／プロジェクト削除時は、該当タブを `useTabStore.getState().closeTask()` で閉じる。
 */
import { create } from "zustand";
import { projectsApi, systemApi, tasksApi, toAppError } from "../api";
import type {
  AppError,
  CreateProjectRequest,
  CreateTaskRequest,
  DeleteTaskOptions,
  EnvironmentInfo,
  Project,
  Task,
  UpdateTaskRequest,
} from "../api";
import { useTabStore } from "./tabStore";

interface ProjectState {
  projects: Project[];
  tasksByProject: Record<string, Task[]>;
  selectedProjectId: string | null;
  error: AppError | null;
  loaded: boolean;
  environment: EnvironmentInfo | null;
  environmentError: AppError | null;

  loadProjects(): Promise<void>;
  loadEnvironment(): Promise<void>;
  selectProject(id: string | null): void;
  addProject(path: string): Promise<Project>;
  createProject(req: CreateProjectRequest): Promise<Project>;
  removeProject(id: string): Promise<void>;

  loadTasks(projectId: string): Promise<void>;
  createTask(req: CreateTaskRequest): Promise<Task>;
  updateTask(req: UpdateTaskRequest): Promise<Task>;
  deleteTask(task: Task, options: DeleteTaskOptions): Promise<void>;
  /** 他ストアが Task を更新したときに反映する（例: pr_number, agent_session_id） */
  upsertTask(task: Task): void;
  findTask(taskId: string): Task | undefined;
  findProject(projectId: string): Project | undefined;
  clearError(): void;
}

const byCreatedAt = <T extends { createdAt: string }>(a: T, b: T) => a.createdAt.localeCompare(b.createdAt);

export const useProjectStore = create<ProjectState>((set, get) => {
  const guard = async <T>(fn: () => Promise<T>): Promise<T> => {
    try {
      return await fn();
    } catch (e) {
      const err = toAppError(e);
      set({ error: err });
      throw err;
    }
  };

  const addProjectToState = (p: Project) =>
    set((s) => ({
      projects: s.projects.some((x) => x.id === p.id) ? s.projects : [...s.projects, p],
      selectedProjectId: p.id,
      tasksByProject: { ...s.tasksByProject, [p.id]: s.tasksByProject[p.id] ?? [] },
    }));

  return {
    projects: [],
    tasksByProject: {},
    selectedProjectId: null,
    error: null,
    loaded: false,
    environment: null,
    environmentError: null,

    loadProjects: () =>
      guard(async () => {
        try {
          const projects = await projectsApi.listProjects();
          set({ projects });
          await Promise.allSettled(projects.map((p) => get().loadTasks(p.id)));
        } finally {
          set({ loaded: true });
        }
      }),
    loadEnvironment: async () => {
      try {
        set({ environment: await systemApi.getEnvironment(), environmentError: null });
      } catch (e) {
        set({ environmentError: toAppError(e) });
      }
    },
    selectProject: (id) => set({ selectedProjectId: id }),
    addProject: (path) =>
      guard(async () => {
        const p = await projectsApi.addProject(path);
        addProjectToState(p);
        await get()
          .loadTasks(p.id)
          .catch(() => {});
        return p;
      }),
    createProject: (req) =>
      guard(async () => {
        const p = await projectsApi.createProject(req);
        addProjectToState(p);
        return p;
      }),
    removeProject: (id) =>
      guard(async () => {
        await projectsApi.removeProject(id);
        const tasks = get().tasksByProject[id] ?? [];
        set((s) => {
          const { [id]: _removed, ...rest } = s.tasksByProject;
          return {
            projects: s.projects.filter((p) => p.id !== id),
            tasksByProject: rest,
            selectedProjectId: s.selectedProjectId === id ? null : s.selectedProjectId,
          };
        });
        const tabs = useTabStore.getState();
        tasks.forEach((t) => tabs.closeTask(t.id));
      }),

    loadTasks: (projectId) =>
      guard(async () => {
        const tasks = await tasksApi.listTasks(projectId);
        set((s) => ({ tasksByProject: { ...s.tasksByProject, [projectId]: [...tasks].sort(byCreatedAt) } }));
      }),
    createTask: (req) =>
      guard(async () => {
        const t = await tasksApi.createTask(req);
        get().upsertTask(t);
        return t;
      }),
    updateTask: (req) =>
      guard(async () => {
        const t = await tasksApi.updateTask(req);
        get().upsertTask(t);
        return t;
      }),
    deleteTask: (task, options) =>
      guard(async () => {
        await tasksApi.deleteTask(task.id, options);
        set((s) => ({
          tasksByProject: {
            ...s.tasksByProject,
            [task.projectId]: (s.tasksByProject[task.projectId] ?? []).filter((t) => t.id !== task.id),
          },
        }));
        useTabStore.getState().closeTask(task.id);
      }),
    upsertTask: (task) =>
      set((s) => {
        const list = s.tasksByProject[task.projectId] ?? [];
        const idx = list.findIndex((t) => t.id === task.id);
        const next = idx >= 0 ? list.map((t) => (t.id === task.id ? task : t)) : [...list, task];
        return { tasksByProject: { ...s.tasksByProject, [task.projectId]: next } };
      }),
    findTask: (taskId) => {
      for (const list of Object.values(get().tasksByProject)) {
        const t = list.find((x) => x.id === taskId);
        if (t) return t;
      }
      return undefined;
    },
    findProject: (projectId) => get().projects.find((p) => p.id === projectId),
    clearError: () => set({ error: null }),
  };
});
