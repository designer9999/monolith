/** Date / value formatters, ported from the MONOLITH design's lib.jsx. */

const MONTHS = [
  "JAN", "FEB", "MAR", "APR", "MAY", "JUN",
  "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

const pad2 = (n: number) => String(n).padStart(2, "0");

/** `2026.06.02` */
export function fmtDate(iso: string): string {
  const d = new Date(iso);
  return `${d.getFullYear()}.${pad2(d.getMonth() + 1)}.${pad2(d.getDate())}`;
}

/** `02 JUN 2026` */
export function fmtDateNice(iso: string): string {
  const d = new Date(iso);
  return `${pad2(d.getDate())} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

/** Relative time: `now`, `5m`, `3h`, `2d`, `4mo`. */
export function rel(iso: string): string {
  const s = (Date.now() - new Date(iso).getTime()) / 1000;
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  if (s < 2592000) return `${Math.floor(s / 86400)}d`;
  return `${Math.floor(s / 2592000)}mo`;
}

/** A bullet mask of bounded length for a hidden secret. */
export function maskOf(v: string): string {
  return "•".repeat(Math.min(Math.max(String(v).length, 6), 22));
}
