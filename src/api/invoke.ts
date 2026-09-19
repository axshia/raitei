import { invoke } from "@tauri-apps/api/core";
import type { AppError } from "./types";

/** invoke の reject 値を AppError に正規化して投げ直す共通ラッパー */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
  }
}

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    return e as AppError;
  }
  return { kind: "command", message: String(e) };
}

export function isAppError(e: unknown): e is AppError {
  return !!e && typeof e === "object" && "kind" in e && "message" in e;
}
