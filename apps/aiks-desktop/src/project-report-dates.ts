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
  kind: "today" | "yesterday" | "week" | "last7" | "month",
  current: Date,
): { from: string; through: string } {
  if (!Number.isFinite(current.getTime())) throw new Error("Invalid local date");
  const day = new Date(current.getFullYear(), current.getMonth(), current.getDate());
  if (kind === "today") {
    const local = calendarDay(day);
    return { from: local, through: local };
  }
  if (kind === "yesterday") {
    const previous = new Date(day.getFullYear(), day.getMonth(), day.getDate() - 1);
    const local = calendarDay(previous);
    return { from: local, through: local };
  }
  if (kind === "last7") {
    const first = new Date(day.getFullYear(), day.getMonth(), day.getDate() - 6);
    return { from: calendarDay(first), through: calendarDay(day) };
  }
  if (kind === "month") {
    return {
      from: calendarDay(new Date(day.getFullYear(), day.getMonth(), 1)),
      through: calendarDay(day),
    };
  }
  // ISO-like Monday-to-Sunday week; only include dates already elapsed.
  const weekday = (day.getDay() + 6) % 7;
  const first = new Date(day.getFullYear(), day.getMonth(), day.getDate() - weekday);
  return { from: calendarDay(first), through: calendarDay(day) };
}
