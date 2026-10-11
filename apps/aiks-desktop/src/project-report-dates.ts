/**
 * S3.2 review date presets: local calendar days, not UTC ISO dates, so a
 * Windows user near midnight does not silently export the wrong workday.
 */
function calendarDay(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return year + "-" + month + "-" + day;
}

export function reportDateRange(
  kind: "today" | "week",
  current: Date,
): { from: string; through: string } {
  if (!Number.isFinite(current.getTime())) throw new Error("Invalid local date");
  const day = new Date(current.getFullYear(), current.getMonth(), current.getDate());
  if (kind === "today") {
    const local = calendarDay(day);
    return { from: local, through: local };
  }
  // ISO-like Monday-to-Sunday week; only include dates already elapsed.
  const weekday = (day.getDay() + 6) % 7;
  const first = new Date(day.getFullYear(), day.getMonth(), day.getDate() - weekday);
  return { from: calendarDay(first), through: calendarDay(day) };
}
