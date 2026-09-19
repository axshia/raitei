/** 小さなドロップダウンメニュー（担当: WS-F）。外側クリック / Esc で閉じる */
import { useEffect, useRef, useState, type ReactNode } from "react";

export interface MenuItem {
  label: ReactNode;
  onSelect(): void;
  danger?: boolean;
  disabled?: boolean;
  hint?: ReactNode;
}

interface MenuProps {
  trigger: (open: boolean) => ReactNode;
  items: (MenuItem | "separator")[];
  align?: "left" | "right";
  label?: string;
}

export function Menu({ trigger, items, align = "right", label = "メニュー" }: MenuProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className="menu" ref={ref}>
      <span
        className="menu-trigger"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((o) => !o);
        }}
      >
        {trigger(open)}
      </span>
      {open && (
        <div className={`menu-popover ${align}`} role="menu" onClick={(e) => e.stopPropagation()}>
          {items.map((it, i) =>
            it === "separator" ? (
              <div key={i} className="menu-sep" />
            ) : (
              <button
                key={i}
                type="button"
                role="menuitem"
                className={`menu-item ${it.danger ? "danger-text" : ""}`}
                disabled={it.disabled}
                onClick={() => {
                  setOpen(false);
                  it.onSelect();
                }}
              >
                <span className="grow">{it.label}</span>
                {it.hint && <span className="subtle">{it.hint}</span>}
              </button>
            ),
          )}
        </div>
      )}
    </div>
  );
}
