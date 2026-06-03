/**
 * Lock-screen overlay: master-password prompt over a blurred scrim. Shown only
 * when an initialized vault is locked. `onUnlock` runs the real `unlock_vault`
 * command and rejects on a wrong password — we await it and surface the error.
 */

import { useEffect, useRef, useState } from "react";

import type { AppError } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { Lbl } from "@/components/ui/primitives";
import { Btn } from "@/components/ui/btn";

export interface LockScreenProps {
  onUnlock: (password: string) => Promise<void>;
  count: number;
}

export function LockScreen({ onUnlock, count }: LockScreenProps) {
  const [pw, setPw] = useState("");
  const [showPw, setShowPw] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const id = window.setTimeout(() => ref.current?.focus(), 60);
    return () => window.clearTimeout(id);
  }, []);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (busy || !pw) return;
    setBusy(true);
    setError(null);
    try {
      await onUnlock(pw);
    } catch (err) {
      const message = (err as AppError)?.message ?? "Could not unlock the vault.";
      setError(message);
      setPw("");
      ref.current?.focus();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-[rgba(7,8,10,0.86)] p-4 backdrop-blur-lg">
      <form onSubmit={submit} className="w-full max-w-[360px] border border-line-2 bg-bg-1 px-5 py-7 sm:px-[30px] sm:py-[34px]">
        <div className="mb-[26px] flex items-center gap-[11px]">
          <div className="relative size-5 bg-acc shadow-[0_0_0_1px_var(--accent-line),0_0_18px_var(--accent-dim)]">
            <div className="absolute inset-1 bg-acc-ink" />
          </div>
          <div className="font-display text-sm font-bold tracking-[0.34em]">MONOLITH</div>
        </div>

        <Lbl className="mb-2.5">Vault locked · {count} secrets</Lbl>
        <h2 className="mb-[22px] font-display text-[22px] font-bold">Master password</h2>

        <div
          className={
            "mb-4 flex items-center gap-2.5 border bg-bg px-3.5 py-3 " +
            (error ? "border-danger" : "border-line-2")
          }
        >
          <Icon name="key" size={15} className="text-acc" />
          <input
            ref={ref}
            type={showPw ? "text" : "password"}
            value={pw}
            onChange={(e) => {
              setPw(e.target.value);
              if (error) setError(null);
            }}
            placeholder="••••••••••••"
            aria-invalid={!!error}
            className="flex-1 border-none bg-transparent font-mono text-sm tracking-[0.2em] text-txt outline-none placeholder:text-txt-4"
          />
          <button
            type="button"
            onClick={() => setShowPw((visible) => !visible)}
            className="grid size-8 place-items-center border border-line bg-bg-2 text-txt-2 transition-colors hover:border-acc hover:text-acc"
            aria-label={showPw ? "Hide master password" : "Show master password"}
          >
            <Icon name={showPw ? "eyeoff" : "eye"} size={15} />
          </button>
        </div>

        {error && (
          <div className="mb-3 flex items-center gap-2 text-[11px] text-danger" role="alert">
            <Icon name="warn" size={12} />
            {error}
          </div>
        )}

        <Btn type="submit" variant="primary" disabled={busy || !pw} className="w-full py-3">
          <Icon name="arrow" size={14} /> {busy ? "Unlocking…" : "Unlock vault"}
        </Btn>

        <Lbl className="mt-[18px] text-center text-txt-4">
          XCHACHA20-POLY1305 · ARGON2ID · LOCAL-ONLY
        </Lbl>
      </form>
    </div>
  );
}
