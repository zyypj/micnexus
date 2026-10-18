import { useEffect, useState, type ReactNode } from "react";
import { Icon } from "./Icon";

export function Toggle({
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: ReactNode;
  hint?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <label className={`toggle-row${disabled ? " disabled" : ""}`}>
      <span className="toggle-text">
        <span className="toggle-label">{label}</span>
        {hint && <span className="toggle-hint">{hint}</span>}
      </span>
      <input
        type="checkbox"
        role="switch"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="switch" aria-hidden />
    </label>
  );
}

export function Modal({ children, onClose, wide }: { children: ReactNode; onClose?: () => void; wide?: boolean }) {
  useEffect(() => {
    if (!onClose) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose?.()}>
      <div className={`modal${wide ? " wide" : ""}`} role="dialog" aria-modal>
        {onClose && (
          <button className="icon-btn modal-close" onClick={onClose} aria-label="Fechar">
            <Icon name="x" />
          </button>
        )}
        {children}
      </div>
    </div>
  );
}

const MODIFIERS = new Set(["Control", "Shift", "Alt", "Meta"]);

/** Turns a keydown into a Tauri accelerator ("Ctrl+Shift+M"). */
export function accelerator(e: KeyboardEvent): string | null {
  if (MODIFIERS.has(e.key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  let key = e.code;
  if (key.startsWith("Key")) key = key.slice(3);
  else if (key.startsWith("Digit")) key = key.slice(5);
  else if (/^F\d+$/.test(key) || ["Space", "Pause", "ScrollLock", "Insert", "Home", "End", "PageUp", "PageDown"].includes(key)) {
    // as-is
  } else if (key.startsWith("Numpad")) key = `Num${key.slice(6)}`;
  else return null;
  // A lone letter would fire while typing anywhere: require a modifier, F-keys aside.
  if (parts.length === 0 && !/^F\d+$/.test(key) && key !== "Pause" && key !== "ScrollLock") return null;
  return [...parts, key].join("+");
}

export function HotkeyInput({ value, onChange }: { value: string | null; onChange: (v: string | null) => void }) {
  const [recording, setRecording] = useState(false);
  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") return setRecording(false);
      const acc = accelerator(e);
      if (acc) {
        onChange(acc);
        setRecording(false);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onChange]);
  return (
    <div className="hotkey">
      <button className={`hotkey-box${recording ? " recording" : ""}`} onClick={() => setRecording(!recording)}>
        {recording ? "Pressione as teclas…" : value ? <kbd>{value.replace(/\+/g, " + ")}</kbd> : "Nenhum"}
      </button>
      {value && !recording && (
        <button className="icon-btn" onClick={() => onChange(null)} title="Remover atalho" aria-label="Remover atalho">
          <Icon name="x" size={16} />
        </button>
      )}
    </div>
  );
}
