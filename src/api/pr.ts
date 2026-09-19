import { call } from "./invoke";
import type { CreatePrRequest, MergePrRequest, PullRequestStatus } from "./types";

/** タスクのブランチの PR（無ければ null） */
export const getPullRequest = (taskId: string) => call<PullRequestStatus | null>("get_pull_request", { taskId });
/** push → gh pr create */
export const createPullRequest = (req: CreatePrRequest) => call<PullRequestStatus>("create_pull_request", { req });
export const mergePullRequest = (req: MergePrRequest) => call<PullRequestStatus>("merge_pull_request", { req });
