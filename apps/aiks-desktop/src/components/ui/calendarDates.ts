/** Local calendar-only values must not be converted to UTC timestamps. */
const pad = (value: number) => String(value).padStart(2, "0");
export function formatLocalIsoDate(date: Date): string {
  return String(date.getFullYear()).padStart(4, "0") + "-" + pad(date.getMonth() + 1) + "-" + pad(date.getDate());
}
export function parseLocalIsoDate(value: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return null;
  // Date(y,...) wrongly maps years 0-99 to the 1900s.
  const date = new Date(2000, Number(match[2]) - 1, Number(match[3]), 12);
  date.setFullYear(Number(match[1]));
  return !Number.isNaN(date.getTime()) && formatLocalIsoDate(date) === value ? date : null;
}
export function localMonthStart(date: Date): Date {
  const month = new Date(date.getTime());
  month.setDate(1);
  month.setHours(12, 0, 0, 0);
  return month;
}
