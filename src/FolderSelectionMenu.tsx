import { useLayoutEffect, useRef, useState } from "react";
import { translate, type Locale } from "./i18n";

export function FolderSelectionMenu({ locale, disabled, onSelect }: { locale: Locale; disabled: boolean; onSelect: () => void }) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const item = useRef<HTMLButtonElement>(null);
  const close = (restoreFocus = false) => {
    setOpen(false);
    if (restoreFocus) trigger.current?.focus();
  };
  useLayoutEffect(() => {
    if (disabled) { setOpen(false); return; }
    if (!open) return;
    const button = trigger.current!, content = menu.current!;
    const position = () => {
      const anchor = button.getBoundingClientRect();
      const bounds = content.getBoundingClientRect();
      content.style.left = `${Math.max(8, Math.min(anchor.right - bounds.width, window.innerWidth - bounds.width - 8))}px`;
      content.style.top = `${Math.max(8, Math.min(anchor.bottom + 4, window.innerHeight - bounds.height - 8))}px`;
    };
    position();
    item.current?.focus({ preventScroll: true });
    const outside = (event: PointerEvent) => {
      if (!content.contains(event.target as Node) && !button.contains(event.target as Node)) close();
    };
    document.addEventListener("pointerdown", outside);
    window.addEventListener("resize", position);
    document.addEventListener("scroll", position, true);
    return () => {
      document.removeEventListener("pointerdown", outside);
      window.removeEventListener("resize", position);
      document.removeEventListener("scroll", position, true);
    };
  }, [open, disabled, locale]);
  return <div className="folder-selection-menu" onBlur={event => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) close();
  }} onKeyDown={event => {
    if (event.key === "Escape" && open) { event.preventDefault(); event.stopPropagation(); close(true); }
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      if (!disabled) { setOpen(true); item.current?.focus(); }
    }
  }}>
    <button ref={trigger} className="quiet folder-selection-trigger" disabled={disabled} aria-label={translate(locale, "entry.more")}
      aria-haspopup="menu" aria-expanded={open && !disabled} aria-controls="folder-selection-menu" onClick={() => setOpen(!open)}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
    </button>
    <div ref={menu} id="folder-selection-menu" className="folder-selection-options" role="menu" aria-label={translate(locale, "entry.more")} hidden={!open || disabled}>
      <button ref={item} role="menuitem" tabIndex={-1} disabled={disabled} onClick={() => { close(true); onSelect(); }}>{translate(locale, "entry.finder")}</button>
    </div>
  </div>;
}
