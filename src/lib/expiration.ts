export type ExpirationTone = "expired" | "soon" | "ok" | "none";

export interface ExpirationInfo {
  tone: ExpirationTone;
  label: string;
  daysLeft: number | null;
}

export const EXPIRATION_PRESETS = [
  { label: "7 days", days: 7 },
  { label: "30 days", days: 30 },
  { label: "60 days", days: 60 },
  { label: "90 days", days: 90 },
] as const;

export function isoDateAfter(days: number): string {
  const d = new Date();
  d.setHours(12, 0, 0, 0);
  d.setDate(d.getDate() + days);
  return d.toISOString().slice(0, 10);
}

export function formatDateOnly(value?: string | null): string {
  if (!value) return "No expiration";
  const date = new Date(`${value}T12:00:00`);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "2-digit",
    year: "numeric",
  });
}

export function expirationInfo(expiresAt?: string | null): ExpirationInfo {
  if (!expiresAt) return { tone: "none", label: "No expiration", daysLeft: null };
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const expires = new Date(`${expiresAt}T00:00:00`);
  if (Number.isNaN(expires.getTime())) {
    return { tone: "none", label: "No expiration", daysLeft: null };
  }
  const daysLeft = Math.ceil((expires.getTime() - today.getTime()) / 86_400_000);
  if (daysLeft < 0) {
    return { tone: "expired", label: "Expired", daysLeft };
  }
  if (daysLeft <= 14) {
    return { tone: "soon", label: `Expires in ${daysLeft}d`, daysLeft };
  }
  return { tone: "ok", label: `Expires ${formatDateOnly(expiresAt)}`, daysLeft };
}

export function isExpirationAttention(expiresAt?: string | null): boolean {
  const tone = expirationInfo(expiresAt).tone;
  return tone === "expired" || tone === "soon";
}
