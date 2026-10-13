/** Domain values, not displayed labels, determine picker identity. */
export interface ChoiceWithValue { value: string }
export function uniqueChoiceValues<T extends ChoiceWithValue>(options: readonly T[]): T[] {
  const seen = new Set<string>();
  return options.filter(option => {
    if (seen.has(option.value)) return false;
    seen.add(option.value);
    return true;
  });
}
/** Fixed-width UTF-16 hex keys preserve case, whitespace, and Unicode through cmdk. */
export function commandValueForOption(value: string): string {
  let key = "choice:";
  for (let i = 0; i < value.length; i += 1) key += value.charCodeAt(i).toString(16).padStart(4, "0");
  return key;
}
/** Radix reserves empty item values. Never alias an actual domain choice. */
export function emptyOptionSentinel(options: readonly ChoiceWithValue[]): string {
  const occupied = new Set(options.map(option => option.value));
  let sentinel = "__aiks_empty__";
  while (occupied.has(sentinel)) sentinel += "_";
  return sentinel;
}
