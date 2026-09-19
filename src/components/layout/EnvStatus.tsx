/** 外部ツールの検出状況（担当: WS-F）。git / gh / claude / codex の有無と gh の認証状態を小さく表示する */
import { useProjectStore } from "../../store/projectStore";
import type { ToolInfo } from "../../api";

function Tool({ label, info, warn }: { label: string; info: ToolInfo | undefined; warn?: string }) {
  const found = !!info?.path;
  const state = !info ? "" : !found ? "danger" : warn ? "warn" : "ok";
  const title = !info
    ? `${label}: 確認中`
    : found
      ? `${label}: ${info.path}${info.version ? `\n${info.version}` : ""}${warn ? `\n${warn}` : ""}`
      : `${label}: 見つかりません（PATH を確認してください）`;
  return (
    <span className="env-tool" title={title}>
      <span className={`dot ${state}`} />
      {label}
    </span>
  );
}

export function EnvStatus() {
  const env = useProjectStore((s) => s.environment);
  const err = useProjectStore((s) => s.environmentError);
  const reload = useProjectStore((s) => s.loadEnvironment);

  return (
    <div className="env-status" onDoubleClick={() => void reload()} title="ダブルクリックで再検出">
      {err ? (
        <span className="error truncate" title={err.message}>
          環境情報を取得できません
        </span>
      ) : (
        <>
          <Tool label="git" info={env?.git} />
          <Tool label="gh" info={env?.gh} warn={env && !env.ghAuthenticated ? "gh auth login が必要です" : undefined} />
          <Tool label="claude" info={env?.claude} />
          <Tool label="codex" info={env?.codex} />
        </>
      )}
    </div>
  );
}
