/**
 * プロジェクト追加（担当: WS-F）: 既存リポジトリ登録 / 新規作成。
 * 仮実装: 既存登録のみディレクトリ選択ダイアログで動く。新規作成フォームは WS-F が実装。
 */
import { open } from "@tauri-apps/plugin-dialog";
import { useProjectStore } from "../../store/projectStore";

export function AddProjectButtons() {
  const addProject = useProjectStore((s) => s.addProject);

  const onAddExisting = async () => {
    const dir = await open({ directory: true, multiple: false, title: "git リポジトリを選択" });
    if (typeof dir === "string") await addProject(dir).catch(() => {});
  };

  return (
    <div style={{ display: "flex", gap: 6, marginBottom: 12 }}>
      <button onClick={onAddExisting}>既存リポジトリを追加</button>
      <button disabled title="WS-F で実装">
        新規作成
      </button>
    </div>
  );
}
