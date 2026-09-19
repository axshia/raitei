/** 新規プロジェクト作成（担当: WS-F）: 親ディレクトリ + 名前 → `create_project`（git init + 初回コミット） */
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toAppError, type AppError } from "../../api";
import { useProjectStore } from "../../store/projectStore";
import { Modal } from "../../components/layout/Modal";

const NAME_RE = /^[A-Za-z0-9._-]+$/;

export function CreateProjectDialog({ onClose }: { onClose(): void }) {
  const createProject = useProjectStore((s) => s.createProject);
  const [parentDir, setParentDir] = useState("");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  const nameInvalid = name !== "" && !NAME_RE.test(name);
  const canSubmit = !!parentDir && !!name && !nameInvalid && !busy;
  const fullPath = parentDir && name ? `${parentDir.replace(/\/+$/, "")}/${name}` : "";

  const pickDir = async () => {
    const dir = await open({ directory: true, multiple: false, title: "作成先の親ディレクトリを選択" });
    if (typeof dir === "string") setParentDir(dir);
  };

  const submit = async () => {
    if (!canSubmit) return;
    setBusy(true);
    setError(null);
    try {
      await createProject({ parentDir, name });
      useProjectStore.getState().clearError();
      onClose();
    } catch (e) {
      setError(toAppError(e));
      useProjectStore.getState().clearError();
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title="新規プロジェクトを作成"
      onClose={onClose}
      onSubmit={submit}
      busy={busy}
      footer={
        <>
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
        <label>作成先（親ディレクトリ）</label>
        <div className="field-row">
          <input
            className="mono"
            style={{ flex: 1 }}
            placeholder="/Users/you/src"
            value={parentDir}
            onChange={(e) => setParentDir(e.target.value)}
          />
          <button type="button" onClick={pickDir}>
            選択…
          </button>
        </div>
      </div>
      <div className="field">
        <label>リポジトリ名</label>
        <input className="mono" placeholder="my-app" value={name} onChange={(e) => setName(e.target.value.trim())} />
        {nameInvalid && <span className="hint error">英数字と . _ - のみ使えます</span>}
      </div>
      <div className="hint">
        {fullPath ? (
          <>
            <span className="mono">{fullPath}</span> に <span className="mono">git init -b main</span> と空の初回コミットを作成して登録します。
          </>
        ) : (
          "新しいディレクトリに git リポジトリを作成して登録します。"
        )}
      </div>
      {error && (
        <div className="banner danger">
          <span className="grow selectable">{error.message}</span>
        </div>
      )}
    </Modal>
  );
}
