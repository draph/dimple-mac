const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

// Match the backend's composed-character count; UTF-16 maxlength would split emoji.
export function countText(text: string): number {
  return [...segmenter.segment(text)].length;
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
