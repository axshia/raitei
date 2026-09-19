import { call } from "./invoke";
import type { CreateProjectRequest, Project } from "./types";

export const listProjects = () => call<Project[]>("list_projects");

/** 既存ローカル git リポジトリを登録 */
export const addProject = (path: string) => call<Project>("add_project", { path });

/** 新規リポジトリを作成して登録 */
export const createProject = (req: CreateProjectRequest) => call<Project>("create_project", { req });

/** 登録解除（ファイルは消さない） */
export const removeProject = (projectId: string) => call<void>("remove_project", { projectId });
