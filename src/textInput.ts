const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

// Match the backend's composed-character count; UTF-16 maxlength would split emoji.
export function countText(text: string): number {
  return [...segmenter.segment(text)].length;
}

// IME composition drafts may exceed the limit until the user commits a candidate.
export function draftText(text: string, composing: boolean, limit = 4): string {
  return composing ? text : limitText(text, limit);
}

// Backend text lags composition by a round trip; adopting it would truncate the active draft.
export function syncedText(text: string | null, composing: boolean, limit = 4): string | null {
  return composing ? null : limitText(text ?? "", limit);
}

export function limitText(text: string, limit = 4): string {
  // shortcut: keeps the leading graphemes, so inserting into a full value drops the tail.
  let limited = "";
  let count = 0;
  for (const { segment } of segmenter.segment(text)) {
    if (count === limit) return limited;
    limited += segment;
    count++;
  }
  return text;
}
