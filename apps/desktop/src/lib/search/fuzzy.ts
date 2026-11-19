// Fuzzy matching for the command palette: a query matches when its letters
// appear in order in the text. Matches at the start, at word starts and in
// runs score higher; gaps score lower. Spaces in the query separate words,
// and every word must match.

const SEPARATORS = " _-./:\\";
const AT_START = 8;
const AT_WORD = 6;
const IN_RUN = 4;
const MAX_GAP_PENALTY = 3;

/** Letters without accents, so "cafe" finds "Café". */
function fold(text: string): string {
  return text.normalize("NFKD").replace(/\p{M}/gu, "");
}

function startsWord(text: string, i: number): boolean {
  if (i === 0) return true;
  const before = text[i - 1] ?? "";
  const here = text[i] ?? "";
  if (SEPARATORS.includes(before)) return true;
  // camelCase: a capital after a small letter starts a word.
  return (
    before !== before.toUpperCase() &&
    here === here.toUpperCase() &&
    here !== here.toLowerCase()
  );
}

/** The best score of `word` (lower case) in `text`, starting at each place its first letter appears. */
function scoreWord(word: string, text: string, lower: string): number | null {
  let best: number | null = null;
  for (
    let start = lower.indexOf(word[0] ?? "");
    start !== -1;
    start = lower.indexOf(word[0] ?? "", start + 1)
  ) {
    let total = 0;
    let at = start;
    let previous = -1;
    for (const letter of word) {
      at = previous === -1 ? start : lower.indexOf(letter, previous + 1);
      if (at === -1) break;
      let points = 1;
      if (at === 0) points += AT_START;
      else if (startsWord(text, at)) points += AT_WORD;
      if (previous !== -1) {
        if (at === previous + 1) points += IN_RUN;
        else points -= Math.min(at - previous - 1, MAX_GAP_PENALTY);
      }
      total += points;
      previous = at;
    }
    if (at !== -1 && (best === null || total > best)) best = total;
  }
  return best;
}

/** How well `query` matches `text`; `null` when it does not. */
export function score(query: string, text: string): number | null {
  const words = fold(query).toLowerCase().trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return 0;
  const folded = fold(text);
  const lower = folded.toLowerCase();
  let total = 0;
  for (const word of words) {
    const s = scoreWord(word, folded, lower);
    if (s === null) return null;
    total += s;
  }
  return total;
}

/**
 * The `limit` best matches of `query`, best first; ties go to the shorter
 * text, then to the earlier item. An empty query keeps the first items.
 */
export function rank<T>(
  query: string,
  items: readonly T[],
  text: (item: T) => string,
  limit: number,
): T[] {
  if (query.trim() === "") return items.slice(0, limit);
  const hits: { item: T; score: number; length: number; index: number }[] = [];
  items.forEach((item, index) => {
    const t = text(item);
    const s = score(query, t);
    if (s !== null) hits.push({ item, score: s, length: t.length, index });
  });
  hits.sort(
    (a, b) => b.score - a.score || a.length - b.length || a.index - b.index,
  );
  return hits.slice(0, limit).map((h) => h.item);
}
