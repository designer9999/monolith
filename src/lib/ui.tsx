/**
 * Shared UI atoms — Tailwind-only. Strength meter, brand service mark, copy
 * button, and the clipboard hook (wired to the real Tauri clipboard auto-clear).
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { cn } from "./utils";
import { Icon } from "./icons";
import { BrandMark, hasBrandMark } from "./brand-marks";
import { copyWithAutoClear } from "./tauri";
import { IconBtn } from "@/components/ui/primitives";

/** Minimal shape needed to render a brand mark. */
export interface MarkLike {
  mono: string;
  color: string;
  slug?: string | null;
  icon?: string | null;
}

/** Copy hook: tracks the last-copied key for the checkmark flash. */
export function useCopy(): [string | true | null, (text: string, key?: string) => void] {
  const [copied, setCopied] = useState<string | true | null>(null);
  const timerRef = useRef<number | null>(null);
  useEffect(() => {
    return () => {
      if (timerRef.current != null) window.clearTimeout(timerRef.current);
    };
  }, []);
  const copy = useCallback((text: string, key?: string) => {
    void copyWithAutoClear(text).then(() => {
      setCopied(key ?? true);
      if (timerRef.current != null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => setCopied(null), 1100);
    });
  }, []);
  return [copied, copy];
}

/** Segmented strength bar (0–100), 5 segments, color by tier. */
export function Strength({ value, w = 54 }: { value?: number | null; w?: number }) {
  if (value == null) return <span className="text-[10px] text-txt-3">—</span>;
  const color = value >= 75 ? "bg-ok" : value >= 45 ? "bg-warn" : "bg-danger";
  const segs = 5;
  const on = Math.round((value / 100) * segs);
  return (
    <div className="flex gap-0.5" style={{ width: w }} title={`Strength ${value}/100`}>
      {Array.from({ length: segs }).map((_, i) => (
        <div key={i} className={cn("h-[5px] flex-1", i < on ? color : "bg-line-2")} />
      ))}
    </div>
  );
}

/**
 * Brand mark on a brutalist tile — rendered fully offline (no network).
 * Priority: real brand logo (bundled Simple Icons, by slug) → named glyph from
 * our line set → service monogram. All tinted in the brand color, like the design.
 */
export function ServiceMark({ tpl, size = 30 }: { tpl?: MarkLike | null; size?: number }) {
  if (!tpl) return null;
  const glyph = Math.round(size * 0.54);
  return (
    <div
      className="grid shrink-0 place-items-center overflow-hidden border border-line-2 bg-bg-2"
      style={{ width: size, height: size }}
    >
      {hasBrandMark(tpl.slug) ? (
        <BrandMark slug={tpl.slug} size={glyph} color={tpl.color} />
      ) : tpl.icon ? (
        <span className="flex" style={{ color: tpl.color }}>
          <Icon name={tpl.icon} size={glyph} stroke={1.7} />
        </span>
      ) : (
        <span className="font-display font-bold" style={{ color: tpl.color, fontSize: size * 0.4 }}>
          {tpl.mono}
        </span>
      )}
    </div>
  );
}

/** Ghost icon button that flashes ✓ when its key is the active copy. */
export function CopyBtn({
  text,
  k,
  copy,
  copied,
}: {
  text: string;
  k: string;
  copy: (t: string, key?: string) => void;
  copied: string | true | null;
}) {
  const active = copied === k;
  return (
    <IconBtn
      active={active}
      title="Copy"
      onClick={(e) => {
        e.stopPropagation();
        copy(text, k);
      }}
    >
      <Icon name={active ? "check" : "copy"} size={13} />
    </IconBtn>
  );
}
