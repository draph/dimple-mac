export type ShortcutSettings = { enabled: boolean; shortcut: string };
export type ShortcutStatus = { settings: ShortcutSettings; active: string[]; uncertain: string[]; saved: boolean; error: unknown | null; open: boolean; capturing: boolean };
export function captureShortcut(event: { code: string; altKey: boolean; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; isComposing: boolean }): string | null {
  if (event.isComposing || !/^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Enter|Tab|Backspace|Delete|Home|End|PageUp|PageDown|Arrow(Up|Down|Left|Right)|Comma|Period|Slash|Semicolon|Quote|BracketLeft|BracketRight|Backslash|Minus|Equal|Backquote)$/.test(event.code)) return null;
  return [...(event.shiftKey ? ["shift"] : []), ...(event.ctrlKey ? ["control"] : []), ...(event.altKey ? ["alt"] : []), ...(event.metaKey ? ["super"] : []), event.code].join("+");
}
export function displayShortcut(value: string): string {
  return value.split("+").map(key => ({ alt: "⌥", control: "⌃", shift: "⇧", super: "⌘", Space: "␣", Enter: "↩", Tab: "⇥", Backspace: "⌫", Delete: "⌦", ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→", Home: "↖", End: "↘", PageUp: "⇞", PageDown: "⇟", Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", BracketLeft: "[", BracketRight: "]", Backslash: "\\", Minus: "−", Equal: "=", Backquote: "`" } as Record<string, string>)[key] ?? key.replace(/^(Key|Digit)/, "")).join("");
}
