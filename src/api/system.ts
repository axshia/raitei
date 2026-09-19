import { call } from "./invoke";
import type { EnvironmentInfo } from "./types";

export const getEnvironment = () => call<EnvironmentInfo>("get_environment");
