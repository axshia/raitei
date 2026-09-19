import { call } from "./invoke";
import type { CreateTaskRequest, DeleteTaskOptions, Task, UpdateTaskRequest } from "./types";

export const listTasks = (projectId: string) => call<Task[]>("list_tasks", { projectId });
export const getTask = (taskId: string) => call<Task>("get_task", { taskId });

/** worktree + ブランチを作成してタスク登録 */
export const createTask = (req: CreateTaskRequest) => call<Task>("create_task", { req });
export const updateTask = (req: UpdateTaskRequest) => call<Task>("update_task", { req });
export const deleteTask = (taskId: string, options: DeleteTaskOptions) =>
  call<void>("delete_task", { taskId, options });
