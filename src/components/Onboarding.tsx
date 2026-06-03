/**
 * MONOLITH — first-run vault setup: master password → Argon2id → envelope →
 * "no recovery" understanding. Ported from the design's onboarding.jsx,
 * Tailwind-only.
 *
 * On "Seal vault" we hand the entered master password back to the caller via
 * `onComplete(masterPassword)` — the caller runs `createVault`. The password is
 * never stored here; it is only kept in component state until the vault is sealed.
 */

import { useState } from "react";

import { Icon } from "@/lib/icons";
import { Lbl, LblText, Chip } from "@/components/ui/primitives";
import { Btn } from "@/components/ui/btn";

/**
 * Lightweight strength scorer (0–100), inlined since `window.MONO` is gone.
 * Mirrors the original heuristic: reward length and character-class diversity.
 */
function estimate(pw: string): number {
  if (!pw) return 0;
  let score = Math.min(pw.length, 16) * 4; // up to 64 for length
  if (/[a-z]/.test(pw)) score += 8;
  if (/[A-Z]/.test(pw)) score += 9;
  if (/[0-9]/.test(pw)) score += 9;
  if (/[^A-Za-z0-9]/.test(pw)) score += 10;
  return Math.min(score, 100);
}

export interface OnboardingFlowProps {
  /** Called with the chosen master password (and whether to seed example data). */
  onComplete: (masterPassword: string, seedDemo: boolean) => void;
  /** Android companion path: scan a desktop QR and import the paired vault. */
  onPairPhone?: () => Promise<void>;
}

export function OnboardingFlow({ onComplete, onPairPhone }: OnboardingFlowProps) {
  const [step, setStep] = useState(0);
  const [pw, setPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [show, setShow] = useState(false);
  const [agreed, setAgreed] = useState(false);
  const [seedDemo, setSeedDemo] = useState(true);
  const [pairBusy, setPairBusy] = useState(false);
  const [pairError, setPairError] = useState<string | null>(null);
  const [pairStatus, setPairStatus] = useState<string | null>(null);

  const est = estimate(pw);
  const reqs: [string, boolean][] = [
    ["12+ characters", pw.length >= 12],
    ["Uppercase letter", /[A-Z]/.test(pw)],
    ["Number", /[0-9]/.test(pw)],
    ["Symbol", /[^A-Za-z0-9]/.test(pw)],
  ];
  const pwOk = reqs.every((r) => r[1]) && pw === confirm;

  // Strength tier → token-mapped Tailwind colors (the only "dynamic" bit left
  // inline is the bar's percentage width).
  const tier = est >= 75 ? "ok" : est >= 45 ? "warn" : "danger";
  const tierText = !pw ? "text-txt-4" : tier === "ok" ? "text-ok" : tier === "warn" ? "text-warn" : "text-danger";
  const tierBg = tier === "ok" ? "bg-ok" : tier === "warn" ? "bg-warn" : "bg-danger";
  const tierLabel = !pw ? "EMPTY" : est >= 75 ? "STRONG" : est >= 45 ? "FAIR" : "WEAK";

  const STEPS: [string, string][] = [
    ["01", "Master key"],
    ["02", "Encryption"],
    ["03", "Confirm"],
  ];
  const next = () => (step < 2 ? setStep(step + 1) : onComplete(pw, seedDemo));
  const canNext = step === 0 ? pwOk : step === 2 ? agreed : true;

  const pairPhone = async () => {
    if (!onPairPhone) return;
    setPairBusy(true);
    setPairError(null);
    setPairStatus("Scan the desktop QR to import this vault.");
    try {
      await onPairPhone();
    } catch (err) {
      setPairError(err instanceof Error ? err.message : "Could not pair this phone.");
      setPairStatus(null);
    } finally {
      setPairBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-[55] flex bg-bg pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)] md:grid md:place-items-center md:p-[30px]">
      <div
        className="pointer-events-none absolute inset-0 opacity-25 [background-image:linear-gradient(var(--line)_1px,transparent_1px),linear-gradient(90deg,var(--line)_1px,transparent_1px)] [background-size:64px_64px] [mask-image:radial-gradient(ellipse_90%_80%_at_50%_40%,#000_30%,transparent_90%)] md:opacity-40"
      />
      <div className="animate-in fade-in relative grid h-full min-h-0 w-full grid-cols-1 bg-bg-1 md:h-[520px] md:max-h-[calc(100vh-60px)] md:w-[760px] md:grid-cols-[240px_1fr] md:border md:border-line-2 md:shadow-[0_40px_100px_rgba(0,0,0,0.6)]">
        {/* rail */}
        <div className="flex flex-none flex-col border-b border-line bg-bg-1/95 px-5 py-3 backdrop-blur md:border-r md:border-b-0 md:px-6 md:py-7">
          <div className="mb-3 flex items-center gap-[11px] md:mb-[34px]">
            <div className="relative size-5 bg-acc shadow-[0_0_0_1px_var(--accent-line),0_0_18px_var(--accent-dim)]">
              <div className="absolute inset-1 bg-acc-ink" />
            </div>
            <div className="font-display text-[14px] font-bold tracking-[0.28em] md:tracking-[0.34em]">MONOLITH</div>
          </div>
          <div className="grid grid-cols-3 gap-1.5 md:flex md:flex-col md:gap-0.5">
            {STEPS.map(([n, l], i) => {
              const on = i === step;
              const done = i < step;
              return (
                <div
                  key={n}
                  className={
                    "flex flex-col items-center gap-1 py-1 md:flex-row md:gap-3 md:py-3 " +
                    (on ? "text-acc" : done ? "text-txt-2" : "text-txt-4")
                  }
                >
                  <span
                    className={
                      "grid size-[26px] flex-none place-items-center border text-[10px] " +
                      (on ? "border-acc-line bg-acc-dim" : "border-line-2 bg-transparent")
                    }
                  >
                    {done ? <Icon name="check" size={12} /> : n}
                  </span>
                  <LblText className="text-current text-[8px] tracking-[0.1em] md:text-[10px] md:tracking-[0.18em]">{l}</LblText>
                </div>
              );
            })}
          </div>
          <div className="hidden flex-1 md:block" />
          <Lbl className="hidden leading-[1.8] text-txt-4 md:block">
            LOCAL-FIRST
            <br />
            NO ACCOUNT · NO CLOUD
            <br />
            NOTHING LEAVES THIS DEVICE
          </Lbl>
        </div>

        {/* content */}
        <div className="flex min-h-0 flex-col">
          <div className="flex-1 overflow-y-auto overflow-x-hidden px-5 py-5 md:px-9 md:py-[34px]">
            {step === 0 && (
              <div className="animate-in fade-in">
                <Lbl className="mb-2.5 text-acc">WELCOME · FIRST RUN</Lbl>
                <h1 className="mb-2 font-display text-[22px] font-bold leading-tight md:text-[26px]">Create your master password</h1>
                <p className="mb-5 max-w-[380px] text-[12px] leading-[1.55] text-txt-2">
                  The single key to everything. It is never stored, never transmitted — only used to
                  derive your encryption key on this machine.
                </p>
                <PwField label="Master password" value={pw} onChange={setPw} show={show} setShow={setShow} />
                <div className="my-2.5 mb-4 flex items-center gap-2.5">
                  <div className="flex h-[5px] flex-1 bg-line-2">
                    <div className={tierBg + " transition-[width] duration-200"} style={{ width: est + "%" }} />
                  </div>
                  <LblText className={"w-16 text-right " + tierText}>{tierLabel}</LblText>
                </div>
                <PwField label="Confirm password" value={confirm} onChange={setConfirm} show={show} setShow={setShow} />
                <div className="mt-4 grid grid-cols-2 gap-x-4 gap-y-2">
                  {reqs.map(([l, ok]) => (
                    <div
                      key={l}
                      className={"flex items-center gap-2 " + (ok ? "text-ok" : "text-txt-4")}
                    >
                      <Icon name={ok ? "check" : "x"} size={12} />
                      <LblText className="text-current">{l}</LblText>
                    </div>
                  ))}
                  <div
                    className={
                      "flex items-center gap-2 " +
                      (pw && pw === confirm ? "text-ok" : "text-txt-4")
                    }
                  >
                    <Icon name={pw && pw === confirm ? "check" : "x"} size={12} />
                    <LblText className="text-current">Passwords match</LblText>
                  </div>
                </div>
                {onPairPhone && (
                  <div className="mt-5 border border-line-2 bg-bg px-4 py-3">
                    <div className="mb-1.5 flex items-center gap-2 text-[13px] leading-snug text-txt">
                      <Icon name="qr" size={14} />
                      Pair this phone to an existing desktop vault
                    </div>
                    <p className="mb-3 text-[11px] leading-[1.5] text-txt-3">
                      Start pairing on the desktop Settings screen, then scan the QR here.
                    </p>
                    {pairError && <div className="mb-3 text-[11px] text-danger">{pairError}</div>}
                    {pairStatus && !pairError && <div className="mb-3 text-[11px] text-txt-3">{pairStatus}</div>}
                    <Btn variant="ghost" className="w-full md:w-auto" onClick={pairPhone} disabled={pairBusy}>
                      <Icon name="qr" size={13} /> {pairBusy ? "Importing vault..." : "Scan desktop QR"}
                    </Btn>
                  </div>
                )}
              </div>
            )}

            {step === 1 && (
              <div className="animate-in fade-in">
                <Lbl className="mb-2.5 text-acc">ENVELOPE ENCRYPTION</Lbl>
                <h1 className="mb-2 font-display text-[23px] font-bold sm:text-[26px]">How your vault is sealed</h1>
                <p className="mb-[22px] max-w-[400px] text-[12px] leading-[1.6] text-txt-2">
                  Your password never encrypts secrets directly. It unwraps a random vault key — so
                  you can change your password without re-encrypting everything.
                </p>
                <div className="flex flex-col gap-px border border-line-2 bg-line">
                  <EnvStep n="01" t="Master password" d="Entered on unlock · held in memory only" color="text-txt" dot="bg-txt" />
                  <EnvStep n="02" t="Argon2id" d="64 MiB · 3 iterations · derives wrapping key" color="text-acc" dot="bg-acc" />
                  <EnvStep n="03" t="XChaCha20-Poly1305" d="Unwraps the random 32-byte vault key" color="text-info" dot="bg-info" />
                  <EnvStep n="04" t="Vault key" d="Encrypts every secret field + TOTP seed" color="text-ok" dot="bg-ok" />
                </div>
                <div className="mt-[18px] flex flex-wrap gap-2">
                  {["XCHACHA20-POLY1305", "ASSOCIATED DATA BOUND", "ZEROIZE ON LOCK", "OFFLINE-ONLY"].map((x) => (
                    <Chip key={x}>{x}</Chip>
                  ))}
                </div>
              </div>
            )}

            {step === 2 && (
              <div className="animate-in fade-in">
                <Lbl className="mb-2.5 text-acc">NO RECOVERY · BY DESIGN</Lbl>
                <h1 className="mb-2 font-display text-[23px] font-bold sm:text-[26px]">Your master password is the only key</h1>
                <p className="mb-[18px] max-w-[400px] text-[12px] leading-[1.6] text-txt-2">
                  There is no account, no email reset, and no backdoor. If you forget your master
                  password, the encrypted data cannot be recovered — that is what keeps it safe.
                </p>
                <div className="mb-4 flex flex-col gap-px border border-line-2 bg-line">
                  {[
                    ["Choose a password you'll remember", "Long and memorable beats short and complex."],
                    ["Store it in a second safe place", "A trusted offline note or a hardware password keeper."],
                    ["Keep an encrypted backup (planned)", "Export will let you restore on another device — coming soon."],
                  ].map(([t, d]) => (
                    <div key={t} className="flex items-start gap-3 bg-bg px-4 py-[13px]">
                      <span className="mt-0.5 flex-none text-acc">
                        <Icon name="check" size={13} />
                      </span>
                      <div>
                        <div className="text-[13px] text-txt">{t}</div>
                        <div className="mt-0.5 text-[11px] text-txt-3">{d}</div>
                      </div>
                    </div>
                  ))}
                </div>
                <button
                  type="button"
                  onClick={() => setAgreed((a) => !a)}
                  className="flex cursor-pointer items-center gap-[11px] border-none bg-transparent p-0 text-txt-2"
                >
                  <span
                    className={
                      "grid size-[18px] flex-none place-items-center border text-acc-ink " +
                      (agreed ? "border-acc bg-acc" : "border-line-2 bg-transparent")
                    }
                  >
                    {agreed && <Icon name="check" size={12} />}
                  </span>
                  <span className="text-[12px]">I understand there is no password recovery.</span>
                </button>
                <button
                  type="button"
                  onClick={() => setSeedDemo((s) => !s)}
                  className="mt-3.5 flex cursor-pointer items-center gap-[11px] border-none bg-transparent p-0 text-txt-2"
                >
                  <span
                    className={
                      "grid size-[18px] flex-none place-items-center border text-acc-ink " +
                      (seedDemo ? "border-acc bg-acc" : "border-line-2 bg-transparent")
                    }
                  >
                    {seedDemo && <Icon name="check" size={12} />}
                  </span>
                  <span className="text-[12px]">Start with example projects (you can delete them anytime).</span>
                </button>
              </div>
            )}
          </div>

          {/* footer */}
          <div className="flex flex-wrap items-center gap-2.5 border-t border-line bg-bg-1 px-5 py-3 md:px-6 md:py-4">
            <LblText className="flex-1 text-txt-4">STEP {step + 1} / 3</LblText>
            {step > 0 && (
              <Btn variant="ghost" className="min-h-11" onClick={() => setStep(step - 1)}>
                <Icon name="back" size={13} /> Back
              </Btn>
            )}
            <Btn variant="primary" className="min-h-11 px-5" disabled={!canNext} onClick={() => canNext && next()}>
              {step < 2 ? (
                <>
                  Continue <Icon name="arrow" size={13} />
                </>
              ) : (
                <>
                  <Icon name="check" size={14} /> Seal vault
                </>
              )}
            </Btn>
          </div>
        </div>
      </div>
    </div>
  );
}

interface PwFieldProps {
  label: string;
  value: string;
  onChange: (v: string) => void;
  show: boolean;
  setShow: (fn: (s: boolean) => boolean) => void;
}

function PwField({ label, value, onChange, show, setShow }: PwFieldProps) {
  return (
    <div>
      <Lbl className="mb-1.5">{label}</Lbl>
      <div className="flex items-center gap-2 border border-line-2 bg-bg">
        <span className="flex pl-3 text-acc">
          <Icon name="key" size={14} />
        </span>
        <input
          type={show ? "text" : "password"}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder=""
          autoComplete="new-password"
          autoCorrect="off"
          autoCapitalize="none"
          spellCheck={false}
          className={
            "min-w-0 flex-1 border-none bg-transparent py-2.5 font-[inherit] text-[14px] text-txt outline-none md:py-[11px] md:text-[13px] " +
            (show ? "tracking-normal" : "tracking-[0.18em]")
          }
        />
        <button
          type="button"
          onClick={() => setShow((s) => !s)}
          className="flex border-none bg-transparent px-3 text-txt-3"
        >
          <Icon name={show ? "eyeoff" : "eye"} size={14} />
        </button>
      </div>
    </div>
  );
}

interface EnvStepProps {
  n: string;
  t: string;
  d: string;
  /** Tailwind text-color token class for the title + accent dot color. */
  color: string;
  /** Tailwind bg-color token class for the marker dot. */
  dot: string;
}

function EnvStep({ n, t, d, color, dot }: EnvStepProps) {
  return (
    <div className="flex flex-wrap items-center gap-3.5 bg-bg px-4 py-[13px]">
      <span className="font-mono text-[11px] tabular-nums text-txt-4">{n}</span>
      <span className={"size-[7px] flex-none " + dot} />
      <span className={"min-w-[150px] flex-1 font-display text-[14px] font-semibold sm:w-[170px] sm:flex-none " + color}>{t}</span>
      <span className="basis-full text-[11px] text-txt-2 sm:basis-auto sm:flex-1">{d}</span>
    </div>
  );
}
