/**
 * TOTP widgets. The rotating code comes from the Rust core (real RFC-6238),
 * fetched on mount and refreshed each period; the countdown animates locally.
 * Visuals ported 1:1 from the design's `Totp` / `TotpChip`.
 */

import { useEffect, useRef, useState } from "react";

import { generateTotp } from "@/lib/tauri";
import type { Item, TotpCode } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { CopyBtn, ServiceMark } from "@/lib/ui";
import { IconBtn } from "@/components/ui/primitives";

interface TotpState {
  code: string;
  remaining: number;
  /** Step length in seconds, as reported by the Rust core. */
  period: number;
  /** True once a real code has been fetched; false while loading or on error. */
  ready: boolean;
  error: string | null;
}

/**
 * Poll the Rust TOTP for a service. Returns the current code, seconds left, the
 * step period (from Rust — not hardcoded), and whether a real code is available.
 * Re-fetches when the step rolls over so the code stays correct.
 */
function useTotp(serviceId: string): TotpState {
  const [state, setState] = useState<TotpState>({
    code: "······",
    remaining: 30,
    period: 30,
    ready: false,
    error: null,
  });
  const remainingRef = useRef(30);

  useEffect(() => {
    let alive = true;
    const fetchCode = async () => {
      try {
        const r: TotpCode = await generateTotp(serviceId);
        if (!alive) return;
        remainingRef.current = r.remaining;
        setState({ code: r.code, remaining: r.remaining, period: r.period, ready: true, error: null });
      } catch (err) {
        if (!alive) return;
        remainingRef.current = 5;
        setState((s) => ({
          ...s,
          remaining: 5,
          ready: false,
          error: err instanceof Error ? err.message : "TOTP unavailable",
        }));
      }
    };
    void fetchCode();
    const id = window.setInterval(() => {
      remainingRef.current -= 1;
      if (remainingRef.current <= 0) {
        void fetchCode();
      } else {
        setState((s) => ({ ...s, remaining: remainingRef.current }));
      }
    }, 1000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [serviceId]);

  return state;
}

/** Full authenticator block shown inside an expanded service panel. */
export function Totp({
  serviceId,
  copy,
  copied,
}: {
  serviceId: string;
  copy: (t: string, k?: string) => void;
  copied: string | true | null;
}) {
  const { code, remaining, period, ready, error } = useTotp(serviceId);
  const frac = remaining / period;
  const danger = remaining <= 5;
  const c = danger ? "text-danger" : "text-acc";
  const stroke = danger ? "stroke-danger" : "stroke-acc";
  const R = 13;
  const C = 2 * Math.PI * R;
  return (
    <div className="flex items-center gap-3.5 border border-line-2 bg-bg px-3.5 py-[13px]">
      <span className={c}>
        <Icon name="refresh" size={15} />
      </span>
      <div className="flex-1">
        <div className="font-mono text-[10px] tracking-[0.18em] uppercase text-txt-3 mb-[5px]">
          Authenticator · TOTP
        </div>
        <div className="font-display tabular-nums text-[26px] font-bold tracking-[0.08em] text-txt">
          {code.slice(0, 3)}
          <span className="text-txt-4 mx-1.5">·</span>
          {code.slice(3)}
        </div>
      </div>
      <div className="relative size-[34px]" title={`${remaining}s`}>
        <svg width="34" height="34" viewBox="0 0 34 34" className="-rotate-90">
          <circle cx="17" cy="17" r={R} fill="none" className="stroke-line-2" strokeWidth="2.5" />
          <circle
            cx="17"
            cy="17"
            r={R}
            fill="none"
            className={`${stroke} transition-[stroke-dashoffset] duration-[250ms] ease-linear`}
            strokeWidth="2.5"
            strokeDasharray={C}
            strokeDashoffset={C * (1 - frac)}
          />
        </svg>
        <span className={`tabular-nums absolute inset-0 grid place-items-center text-[10px] ${c}`}>
          {remaining}
        </span>
      </div>
      {ready ? (
        <CopyBtn text={code} k="totp" copy={copy} copied={copied} />
      ) : (
        <IconBtn disabled className="cursor-not-allowed opacity-40" title={error ?? "No code yet"}>
          <Icon name="copy" size={13} />
        </IconBtn>
      )}
    </div>
  );
}

/** Compact live-code chip for the home authenticator strip. Tap to copy. */
export function TotpChip({
  item,
  copy,
  copied,
}: {
  item: Item;
  copy: (t: string, k?: string) => void;
  copied: string | true | null;
}) {
  const { code, remaining, period, ready, error } = useTotp(item.id);
  const frac = remaining / period;
  const danger = remaining <= 5;
  const stroke = danger ? "stroke-danger" : "stroke-acc";
  const c = danger ? "text-danger" : "text-acc";
  const k = `tc_${item.id}`;
  const hit = copied === k;
  const R = 10;
  const CC = 2 * Math.PI * R;
  return (
    <button
      onClick={() => ready && copy(code, k)}
      disabled={!ready}
      title={ready ? `Copy code · ${item.title}` : error ?? "No code yet"}
      className="flex items-center gap-3 border border-line-2 bg-bg px-3 py-2.5 min-w-[228px] cursor-pointer text-left transition-colors duration-[120ms] hover:border-line-3 disabled:cursor-not-allowed disabled:opacity-50"
    >
      <ServiceMark tpl={{ mono: item.mono, color: item.color, slug: item.slug, icon: item.icon }} size={32} />
      <div className="flex-1 min-w-0">
        <div className="font-mono text-[10px] tracking-[0.18em] uppercase text-txt-4 overflow-hidden text-ellipsis whitespace-nowrap mb-[3px]">
          {item.projectName}
        </div>
        <div
          className={`font-display tabular-nums text-[18px] font-bold tracking-[0.08em] ${hit ? "text-ok" : "text-txt"}`}
        >
          {hit ? "COPIED" : `${code.slice(0, 3)} ${code.slice(3)}`}
        </div>
      </div>
      <div className="relative size-[26px]">
        <svg width="26" height="26" viewBox="0 0 26 26" className="-rotate-90">
          <circle cx="13" cy="13" r={R} fill="none" className="stroke-line-2" strokeWidth="2" />
          <circle
            cx="13"
            cy="13"
            r={R}
            fill="none"
            className={`${stroke} transition-[stroke-dashoffset] duration-500 ease-linear`}
            strokeWidth="2"
            strokeDasharray={CC}
            strokeDashoffset={CC * (1 - frac)}
          />
        </svg>
        <span className={`tabular-nums absolute inset-0 grid place-items-center text-[9px] ${c}`}>
          {remaining}
        </span>
      </div>
    </button>
  );
}
