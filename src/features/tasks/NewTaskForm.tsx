/** 新規タスク作成フォーム（担当: WS-F）。仮実装: タイトル + ブランチ + エージェント */
import { useState } from "react";
import type { AgentKind, Project } from "../../api";
import { useProjectStore } from "../../store/projectStore";
import { useTabStore } from "../../store/tabStore";

export function NewTaskForm({ project }: { project: Project }) {
  const createTask = useProjectStore((s) => s.createTask);
  const openTask = useTabStore((s) => s.openTask);
  const [title, setTitle] = useState("");
  const [branch, setBranch] = useState("");
  const [agent, setAgent] = useState<AgentKind>("claude");

  const submit = async () => {
    const t = await createTask({ projectId: project.id, title, branch, agent }).catch(() => null);
    if (t) {
      setTitle("");
      setBranch("");
      openTask(t.id);
    }
  };

  return (
    <div style={{ display: "grid", gap: 4 }}>
      <input placeholder="タスク名" value={title} onChange={(e) => setTitle(e.target.value)} />
      <input placeholder="ブランチ名" value={branch} onChange={(e) => setBranch(e.target.value)} />
      <div style={{ display: "flex", gap: 4 }}>
        <select value={agent} onChange={(e) => setAgent(e.target.value as AgentKind)}>
          <option value="claude">claude</option>
          <option value="codex">codex</option>
        </select>
        <button disabled={!title || !branch} onClick={submit}>
          タスク作成
        </button>
      </div>
    </div>
  );
}
