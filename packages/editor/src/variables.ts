export interface VariableRange {
  from: number;
  to: number;
  name: string;
}

export interface VariableState {
  defined(name: string): boolean;
  describe(name: string): { shown: string; source: string } | null;
}

/** Finds `{{name}}` placeholders. The name keeps `vault:` and `$` prefixes as written. */
export function variableRanges(text: string): VariableRange[] {
  const ranges: VariableRange[] = [];
  const pattern = /\{\{\s*([^{}\s](?:[^{}]*[^{}\s])?)\s*\}\}/g;
  for (const match of text.matchAll(pattern)) {
    ranges.push({
      from: match.index,
      to: match.index + match[0].length,
      name: match[1] ?? "",
    });
  }
  return ranges;
}
