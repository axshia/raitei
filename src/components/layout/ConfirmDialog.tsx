/** 確認ダイアログ（担当: WS-F）。`onConfirm` が reject したらメッセージを表示して開いたままにする */
import { useState, type ReactNode } from "react";
import { toAppError, type AppError } from "../../api";
import { Modal } from "./Modal";

interface ConfirmDialogProps {
  title: ReactNode;
  children: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  onConfirm(): Promise<void>;
  onClose(): void;
}

export function ConfirmDialog({ title, children, confirmLabel, danger, onConfirm, onClose }: ConfirmDialogProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await onConfirm();
      onClose();
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={title}
      onClose={onClose}
      onSubmit={submit}
      busy={busy}
      footer={
        <>
          <button type="button" onClick={onClose} disabled={busy}>
            キャンセル
          </button>
          <button type="submit" className={danger ? "danger" : "primary"} disabled={busy}>
            {busy && <span className="spinner" />}
            {confirmLabel}
          </button>
        </>
      }
    >
      {children}
      {error && (
        <div className="banner danger">
          <span className="grow selectable">{error.message}</span>
        </div>
      )}
    </Modal>
  );
}
