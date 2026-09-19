/**
 * プロジェクト・タスク一覧ストア（担当: WS-F）。
 * 公開する state / action 名は契約。内部実装は自由に変更してよい。
 */
import { create } from "zustand";
import { projectsApi, tasksApi, toAppError } from "../api";
import type { AppError, CreateProjectRequest, CreateTaskRequest, DeleteTaskOptions, Project, Task } from "../api";

interface ProjectState {
  projects: Project[];
  tasksByProject: Record<string, Task[]>;
  selectedProjectId: string | null;
  error: AppError | null;

  loadProjects(): Promise<void>;
  selectProject(id: string | null): void;
  addProject(path: string): Promise<Project>;
  createProject(req: CreateProjectRequest): Promise<Project>;
  removeProject(id: string): Promise<void>;

  loadTasks(projectId: string): Promise<void>;
  createTask(req: CreateTaskRequest): Promise<Task>;
  deleteTask(task: Task, options: DeleteTaskOptions): Promise<void>;
  /** 他ストアが Task を更新したときに反映する（例: pr_number, agent_session_id） */
  upsertTask(task: Task): void;
  findTask(taskId: string): Task | undefined;
  clearError(): void;
}

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

  return {
    projects: [],
    tasksByProject: {},
    selectedProjectId: null,
    error: null,

    loadProjects: () =>
      guard(async () => {
        const projects = await projectsApi.listProjects();
        set({ projects });
        await Promise.all(projects.map((p) => get().loadTasks(p.id)));
      }),
    selectProject: (id) => set({ selectedProjectId: id }),
    addProject: (path) =>
      guard(async () => {
        const p = await projectsApi.addProject(path);
        set((s) => ({ projects: [...s.projects, p], selectedProjectId: p.id, tasksByProject: { ...s.tasksByProject, [p.id]: [] } }));
        return p;
      }),
    createProject: (req) =>
      guard(async () => {
        const p = await projectsApi.createProject(req);
        set((s) => ({ projects: [...s.projects, p], selectedProjectId: p.id, tasksByProject: { ...s.tasksByProject, [p.id]: [] } }));
        return p;
      }),
    removeProject: (id) =>
      guard(async () => {
        await projectsApi.removeProject(id);
        set((s) => {
          const { [id]: _removed, ...rest } = s.tasksByProject;
          return {
            projects: s.projects.filter((p) => p.id !== id),
            tasksByProject: rest,
            selectedProjectId: s.selectedProjectId === id ? null : s.selectedProjectId,
          };
        });
      }),

    loadTasks: (projectId) =>
      guard(async () => {
        const tasks = await tasksApi.listTasks(projectId);
        set((s) => ({ tasksByProject: { ...s.tasksByProject, [projectId]: tasks } }));
      }),
    createTask: (req) =>
      guard(async () => {
        const t = await tasksApi.createTask(req);
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
      }),
    upsertTask: (task) =>
      set((s) => {
        const list = s.tasksByProject[task.projectId] ?? [];
        const idx = list.findIndex((t) => t.id === task.id);
        const next = idx >= 0 ? list.map((t) => (t.id === task.id ? task : t)) : [...list, task];
        return { tasksByProject: { ...s.tasksByProject, [task.projectId]: next } };
      }),
    findTask: (taskId) => Object.values(get().tasksByProject).flat().find((t) => t.id === taskId),
    clearError: () => set({ error: null }),
  };
});
