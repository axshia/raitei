/**
 * 汎用モーダル（担当: WS-F）。Esc / 背景クリックで閉じる。
 * フッターのボタンは `footer` に渡す。フォーム送信は `onSubmit` を渡すと Enter で発火する。
 */
import { useEffect, useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";

/** 開いているモーダルの積み順（Esc は最前面だけが処理する） */
const stack: string[] = [];

interface ModalProps {
  title: ReactNode;
  onClose(): void;
  children: ReactNode;
  footer?: ReactNode;
  onSubmit?(): void;
  width?: number;
  /** 処理中は閉じられないようにする */
  busy?: boolean;
}

export function Modal({ title, onClose, children, footer, onSubmit, width = 440, busy }: ModalProps) {
  const ref = useRef<HTMLFormElement>(null);
  const id = useId();

  useEffect(() => {
    stack.push(id);
    return () => {
      stack.splice(stack.indexOf(id), 1);
    };
  }, [id]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !busy && stack[stack.length - 1] === id) {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose, busy, id]);

  useEffect(() => {
    // 最初の入力欄にフォーカス
    const el = ref.current?.querySelector<HTMLElement>("input:not([type=checkbox]):not([type=radio]), textarea, select");
    el?.focus();
  }, []);

  // フォームの入れ子や親の overflow の影響を受けないよう body 直下に出す
  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
      <form
        ref={ref}
        className="modal"
        style={{ width }}
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          e.stopPropagation(); // portal でも React の合成イベントは親モーダルへ伝播するため
          if (!busy) onSubmit?.();
        }}
      >
        <header className="modal-header">
          <span className="truncate">{title}</span>
          <button type="button" className="ghost icon" onClick={onClose} disabled={busy} aria-label="閉じる">
            ×
          </button>
        </header>
        <div className="modal-body">{children}</div>
        {footer && <footer className="modal-footer">{footer}</footer>}
      </form>
    </div>,
    document.body,
  );
}
