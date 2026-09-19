/**
 * 新規タスク作成ダイアログ（担当: WS-F）。
 * タスク = worktree + ブランチ + エージェントセッション。`create_task` が worktree とブランチを作る。
 * 入力: タイトル / ブランチ（タイトルから自動提案）/ base ブランチ / エージェント種別 / 権限。
 */
import { useMemo, useState } from "react";
import { toAppError, type AgentKind, type AppError, type PermissionLevel, type Project } from "../../api";
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";
import { Modal } from "../../components/layout/Modal";
import { shortenHome, suggestBranch, validateBranch, worktreePathFor } from "./branch";

const DEFAULTS_KEY = "raitei.newTask.defaults";

interface Defaults {
  agent: AgentKind;
  permission: PermissionLevel;
}

function loadDefaults(): Defaults {
  try {
    const v = JSON.parse(localStorage.getItem(DEFAULTS_KEY) ?? "{}") as Partial<Defaults>;
    return {
      agent: v.agent === "codex" ? "codex" : "claude",
      permission: v.permission === "full" ? "full" : "safe",
    };
  } catch {
    return { agent: "claude", permission: "safe" };
  }
}

const AGENTS: { key: AgentKind; label: string; desc: string }[] = [
  { key: "claude", label: "Claude Code", desc: "claude -p（stream-json）" },
  { key: "codex", label: "Codex", desc: "codex exec --json" },
];

const PERMISSIONS: { key: PermissionLevel; label: string; desc: Record<AgentKind, string> }[] = [
  {
    key: "safe",
    label: "安全",
    desc: { claude: "--permission-mode acceptEdits（編集は自動承認）", codex: "sandbox: workspace-write" },
  },
  {
    key: "full",
    label: "フル",
    desc: { claude: "--permission-mode bypassPermissions", codex: "--dangerously-bypass-approvals-and-sandbox" },
  },
];

export function NewTaskDialog({ project, onClose }: { project: Project; onClose(): void }) {
  const createTask = useProjectStore((s) => s.createTask);
  const env = useProjectStore((s) => s.environment);
  const openTask = useTabStore((s) => s.openTask);
  const initial = useMemo(loadDefaults, []);

  const [title, setTitle] = useState("");
  const [branch, setBranch] = useState("");
  const [branchEdited, setBranchEdited] = useState(false);
  const [baseBranch, setBaseBranch] = useState(project.defaultBranch);
  const [agent, setAgent] = useState<AgentKind>(initial.agent);
  const [permission, setPermission] = useState<PermissionLevel>(initial.permission);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  const effectiveBranch = branchEdited ? branch : title.trim() ? suggestBranch(title) : "";
  const branchError = validateBranch(effectiveBranch);
  const baseError = validateBranch(baseBranch.trim());
  const canSubmit = !!title.trim() && !!effectiveBranch && !branchError && !baseError && !busy;
  const agentMissing = env && !env[agent].path;

  const submit = async () => {
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      const t = await createTask({
        projectId: project.id,
        title: title.trim(),
        branch: effectiveBranch,
        baseBranch: baseBranch.trim() || null,
        agent,
        permission,
      });
      localStorage.setItem(DEFAULTS_KEY, JSON.stringify({ agent, permission } satisfies Defaults));
      openTask(t.id, "chat");
      onClose();
    } catch (e) {
      setError(toAppError(e));
    } finally {
      useProjectStore.getState().clearError();
      setBusy(false);
    }
  };

  return (
    <Modal
      title={
        <>
          新しいタスク <span className="muted">— {project.name}</span>
        </>
      }
      width={520}
      onClose={onClose}
      onSubmit={submit}
      busy={busy}
      footer={
        <>
          <span className="hint" style={{ marginRight: "auto" }}>
            worktree とブランチを作成してタブを開きます
          </span>
          <button type="button" onClick={onClose} disabled={busy}>
            キャンセル
          </button>
          <button type="submit" className="primary" disabled={!canSubmit}>
            {busy && <span className="spinner" />}作成
          </button>
        </>
      }
    >
      <div className="field">
        <label>タイトル</label>
        <input placeholder="例: ログイン画面のバリデーション修正" value={title} onChange={(e) => setTitle(e.target.value)} />
      </div>

      <div className="form-grid">
        <div className="field">
          <label>ブランチ</label>
          <input
            className="mono"
            placeholder="task/..."
            value={effectiveBranch}
            onChange={(e) => {
              setBranchEdited(true);
              setBranch(e.target.value.trim());
            }}
          />
          {branchError ? (
            <span className="hint error">{branchError}</span>
          ) : (
            <span className="hint">既存ブランチ名ならそれを checkout します</span>
          )}
        </div>
        <div className="field">
          <label>base ブランチ</label>
          <input className="mono" value={baseBranch} onChange={(e) => setBaseBranch(e.target.value)} />
          {baseError ? <span className="hint error">{baseError}</span> : <span className="hint">分岐元・PR のマージ先</span>}
        </div>
      </div>

      <div className="field">
        <span className="label">エージェント</span>
        <div className="choice-group">
          {AGENTS.map((a) => {
            const missing = env && !env[a.key].path;
            return (
              <label key={a.key} className={`choice ${agent === a.key ? "checked" : ""}`}>
                <input type="radio" name="agent" checked={agent === a.key} onChange={() => setAgent(a.key)} />
                <span className="choice-body">
                  <span className="choice-title">
                    <span className={`agent-mark ${a.key}`}>{a.key === "claude" ? "CL" : "CX"}</span>
                    {a.label}
                    {missing && <span className="badge danger">未検出</span>}
                  </span>
                  <span className="hint mono">{a.desc}</span>
                </span>
              </label>
            );
          })}
        </div>
      </div>

      <div className="field">
        <span className="label">権限</span>
        <div className="choice-group">
          {PERMISSIONS.map((p) => (
            <label key={p.key} className={`choice ${permission === p.key ? "checked" : ""}`}>
              <input type="radio" name="permission" checked={permission === p.key} onChange={() => setPermission(p.key)} />
              <span className="choice-body">
                <span className="choice-title">
                  {p.label}
                  {p.key === "full" && <span className="badge warn">注意</span>}
                </span>
                <span className="hint mono">{p.desc[agent]}</span>
              </span>
            </label>
          ))}
        </div>
      </div>

      {effectiveBranch && !branchError && (
        <div className="hint">
          worktree: <span className="mono">{shortenHome(worktreePathFor(project.repoPath, effectiveBranch))}</span>
        </div>
      )}
      {agentMissing && (
        <div className="banner warn">
          <span className="grow">{agent} コマンドが PATH に見つかりません。タスクは作成できますが、メッセージ送信は失敗します。</span>
        </div>
      )}
      {error && (
        <div className="banner danger">
          <span className="grow selectable">{error.message}</span>
        </div>
      )}
    </Modal>
  );
}
