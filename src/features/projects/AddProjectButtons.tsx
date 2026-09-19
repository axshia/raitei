/**
 * プロジェクト追加（担当: WS-F）: 既存リポジトリ登録（ディレクトリ選択）/ 新規作成（親ディレクトリ + 名前）。
 */
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useProjectStore } from "../../store/projectStore";
import { Menu } from "../../components/layout/Menu";
import { CreateProjectDialog } from "./CreateProjectDialog";

export function AddProjectButtons() {
  const addProject = useProjectStore((s) => s.addProject);
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState(false);

  const onAddExisting = async () => {
    const dir = await open({ directory: true, multiple: false, title: "登録する git リポジトリを選択" });
    if (typeof dir !== "string") return;
    setBusy(true);
    try {
      await addProject(dir);
    } catch {
      // エラーは projectStore.error に入り、サイドバー下部に表示される
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Menu
        align="right"
        label="プロジェクトを追加"
        trigger={() => (
          <button className="sm" disabled={busy} title="プロジェクトを追加">
            {busy ? <span className="spinner" /> : "+"} 追加
          </button>
        )}
        items={[
          { label: "既存の git リポジトリを登録…", onSelect: () => void onAddExisting() },
          { label: "新規リポジトリを作成…", onSelect: () => setCreating(true) },
        ]}
      />
      {creating && <CreateProjectDialog onClose={() => setCreating(false)} />}
    </>
  );
}
