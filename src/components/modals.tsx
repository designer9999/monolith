/**
 * MONOLITH — modals: the service catalog (template picker + configure step),
 * the create-project dialog, and the shared Modal shell + FormField. Ported 1:1
 * from the design's modals.jsx. Templates come from the real vault catalog
 * (`listTemplates`), and the collected values are handed back to the caller so
 * it can drive `addService` / `createProject`.
 */

import { Fragment, useEffect, useRef, useState, type ClipboardEvent as ReactClipboardEvent } from "react";

import { listTemplates, revealField } from "@/lib/tauri";
import type {
  AppError,
  CreateProjectInput,
  Environment,
  Project,
  Service,
  ServiceFieldInput,
  Template,
  UpdateProjectInput,
} from "@/lib/types";
import { Icon } from "@/lib/icons";
import { EXPIRATION_PRESETS, formatDateOnly, isoDateAfter } from "@/lib/expiration";
import {
  credentialAssistFor,
  credentialFieldHint,
  credentialPlaceholder,
  runCredentialAutofill,
  type CredentialAutofillResult,
} from "@/lib/credential-autofill";
import { decodeTotpQrImage, extractTotpSecret } from "@/lib/totp-qr";
import { ServiceMark } from "@/lib/ui";
import { Btn } from "@/components/ui/btn";
import { Lbl } from "@/components/ui/primitives";

/** Values gathered in the configure step, ready to become an `AddServiceInput`. */
export interface ServiceDraft {
  label?: string;
  env?: Environment;
  expiresAt?: string;
  totpSecret?: string;
  fields: ServiceFieldInput[];
}

const ENVIRONMENTS: Environment[] = ["production", "staging", "dev", "all"];
const ENV_LABEL: Record<Environment, string> = {
  production: "PROD",
  staging: "STAGING",
  dev: "DEV",
  all: "ALL ENV",
};

type AssistState = {
  field: string | null;
  busy: boolean;
  message: string | null;
  note: string | null;
  error: string | null;
};

const EMPTY_ASSIST: AssistState = {
  field: null,
  busy: false,
  message: null,
  note: null,
  error: null,
};

function applyAutofillValues(
  current: Record<string, string>,
  result: CredentialAutofillResult,
): Record<string, string> {
  if (!result.values) return current;
  return { ...current, ...result.values };
}

/* ---------- shared service controls ---------- */

function EnvironmentPicker({
  value,
  onChange,
}: {
  value: Environment;
  onChange: (value: Environment) => void;
}) {
  return (
    <div className="mb-3.5">
      <Lbl className="mb-1.5">Environment</Lbl>
      <div className="flex border border-line-2">
        {ENVIRONMENTS.map((opt, i) => (
          <button
            key={opt}
            type="button"
            onClick={() => onChange(opt)}
            className={
              "px-[13px] py-[7px] font-mono text-[10px] tracking-[0.1em] uppercase " +
              (i ? "border-l border-line " : "") +
              (value === opt ? "bg-bg-3 text-acc" : "bg-transparent text-txt-3")
            }
          >
            {ENV_LABEL[opt]}
          </button>
        ))}
      </div>
    </div>
  );
}

function ExpirationPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const presetValues = EXPIRATION_PRESETS.map((p) => isoDateAfter(p.days));
  const custom = value && !presetValues.includes(value);
  const noExpirationSelected = !value;
  return (
    <div className="mb-3.5">
      <Lbl className="mb-1.5">Expiration</Lbl>
      <div className="grid gap-px bg-line [grid-template-columns:repeat(auto-fit,minmax(122px,1fr))]">
        {EXPIRATION_PRESETS.map((p) => {
          const date = isoDateAfter(p.days);
          const selected = value === date;
          return (
            <button
              key={p.days}
              type="button"
              onClick={() => onChange(date)}
              className={`relative bg-bg px-3 py-2 text-left font-mono text-[10px] uppercase tracking-[0.1em] ${
                selected ? "text-acc" : "text-txt-3"
              }`}
            >
              {selected && <span className="absolute left-0 top-0 h-full w-[2px] bg-acc" />}
              <span className={`block text-[11px] ${selected ? "text-acc" : "text-txt"}`}>{p.label}</span>
              {formatDateOnly(date)}
            </button>
          );
        })}
        <button
          type="button"
          onClick={() => onChange("")}
          className={`relative bg-bg px-3 py-2 text-left font-mono text-[10px] uppercase tracking-[0.1em] ${
            noExpirationSelected ? "text-acc" : "text-txt-3"
          }`}
        >
          {noExpirationSelected && <span className="absolute left-0 top-0 h-full w-[2px] bg-acc" />}
          <span className={`block text-[11px] ${noExpirationSelected ? "text-acc" : "text-txt"}`}>No expiration</span>
          Manual rotation
        </button>
      </div>
      <input
        type="date"
        value={custom ? value : ""}
        onChange={(e) => onChange(e.target.value)}
        className="mt-2 w-full border border-line-2 bg-bg px-3 py-2 font-mono text-[11px] text-txt outline-none"
        aria-label="Custom expiration date"
      />
    </div>
  );
}

/* ---------- modal shell ---------- */

function Modal({
  title,
  sub,
  onClose,
  children,
  footer,
  wide,
}: {
  title: string;
  sub?: string;
  onClose: () => void;
  children: React.ReactNode;
  footer?: React.ReactNode;
  wide?: boolean;
}) {
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const focusable = () =>
      Array.from(
        panelRef.current?.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      ).filter((el) => !el.hasAttribute("disabled"));

    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      // Focus trap: keep Tab cycling within the dialog.
      if (e.key === "Tab") {
        const els = focusable();
        if (els.length === 0) return;
        const first = els[0];
        const last = els[els.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener("keydown", h);
    // Move focus into the dialog on open.
    focusable()[0]?.focus();
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);

  return (
    <div
      onMouseDown={onClose}
      className="fixed inset-0 z-[45] grid place-items-end bg-[rgba(7,8,10,0.74)] p-3 backdrop-blur-[6px] sm:place-items-center sm:p-[30px]"
    >
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onMouseDown={(e) => e.stopPropagation()}
        className={`animate-in fade-in flex max-h-[92vh] w-full flex-col border border-line-2 bg-bg-1 shadow-[0_30px_90px_rgba(0,0,0,0.6)] sm:max-h-[86vh] ${
          wide ? "sm:w-[720px]" : "sm:w-[520px]"
        }`}
      >
        <div className="flex items-center justify-between border-b border-line px-4 py-4 sm:px-[22px] sm:py-[18px]">
          <div>
            <div className="font-display text-[18px] font-bold">{title}</div>
            {sub && <Lbl className="mt-[5px] text-txt-3">{sub}</Lbl>}
          </div>
          <Btn variant="ghost" size="icon" onClick={onClose}>
            <Icon name="x" size={14} />
          </Btn>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
        {footer && (
          <div className="flex flex-wrap justify-end gap-2.5 border-t border-line px-4 py-4 sm:px-[22px]">{footer}</div>
        )}
      </div>
    </div>
  );
}

/* ---------- service catalog with template field preview ---------- */

export function ServiceCatalog({
  project,
  templates,
  onClose,
  onAdd,
}: {
  project?: Project | null;
  templates?: Template[];
  onClose: () => void;
  onAdd?: (templateId: string, values: ServiceDraft) => void | Promise<void>;
}) {
  const [list, setList] = useState<Template[]>(templates ?? []);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<Template | null>(null);
  const [vals, setVals] = useState<Record<string, string>>({});
  const [env, setEnv] = useState<Environment>("all");
  const [expiresAt, setExpiresAt] = useState("");
  const [loading, setLoading] = useState(!templates);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [assist, setAssist] = useState<AssistState>(EMPTY_ASSIST);

  useEffect(() => {
    if (templates) {
      setList(templates);
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    setError(null);
    void listTemplates()
      .then((t) => {
        if (alive) setList(t);
      })
      .catch((err) => {
        if (alive) setError((err as AppError)?.message ?? "Couldn't load templates.");
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [templates]);

  useEffect(() => {
    setAssist(EMPTY_ASSIST);
  }, [sel?.id]);

  const filtered = list.filter(
    (t) =>
      !q ||
      t.name.toLowerCase().includes(q.toLowerCase()) ||
      t.group.toLowerCase().includes(q.toLowerCase()),
  );
  const groups: Record<string, Template[]> = {};
  filtered.forEach((t) => {
    (groups[t.group] = groups[t.group] || []).push(t);
  });

  if (sel) {
    const t = sel;
    const runAssist = async (fieldLabel: string) => {
      if (assist.busy) return;
      setAssist({ field: fieldLabel, busy: true, message: null, note: null, error: null });
      try {
        const result = await runCredentialAutofill(t.id, fieldLabel, vals[fieldLabel] || "");
        setVals((current) => ({
          ...applyAutofillValues(current, result),
          ...(current.__label || !result.label ? {} : { __label: result.label }),
        }));
        setAssist({
          field: fieldLabel,
          busy: false,
          message: result.message,
          note: result.note ?? null,
          error: null,
        });
      } catch (err) {
        setAssist({
          field: fieldLabel,
          busy: false,
          message: null,
          note: null,
          error: (err as Error)?.message ?? "Could not fetch token metadata.",
        });
      }
    };
    const submit = async () => {
      if (submitting) return;
      const fields: ServiceFieldInput[] = t.fields
        .filter((f) => (vals[f.label] || "").length > 0)
        .map((f) => ({ label: f.label, value: vals[f.label] || "" }));
      try {
        setSubmitting(true);
        setError(null);
        await onAdd?.(t.id, {
          label: vals.__label?.trim() || undefined,
          env,
          expiresAt: expiresAt || undefined,
          totpSecret: t.totp ? vals.__totp?.trim() || undefined : undefined,
          fields,
        });
      } catch (err) {
        setError((err as AppError)?.message ?? "Couldn't add the service.");
      } finally {
        setSubmitting(false);
      }
    };
    return (
      <Modal
        title="Configure service"
        sub={`${t.name} · template auto-filled`}
        onClose={onClose}
        footer={
          <>
            <Btn variant="ghost" onClick={() => setSel(null)} disabled={submitting}>
              <Icon name="back" size={13} /> Back
            </Btn>
            <Btn variant="primary" onClick={() => void submit()} disabled={submitting}>
              <Icon name="check" size={14} /> {submitting ? "Adding…" : `Add to ${project?.name || "project"}`}
            </Btn>
          </>
        }
      >
        <div className="px-4 py-5 sm:px-[22px]">
          <div className="mb-1.5 flex items-center gap-3.5">
            <ServiceMark tpl={t} size={44} />
            <div>
              <div className="font-display text-[17px] font-semibold">{t.name}</div>
              <Lbl className="mt-1 text-txt-3">
                {t.group} · {t.fields.length} FIELDS{t.totp ? " · TOTP" : ""}
              </Lbl>
            </div>
          </div>
          <div className="my-4 flex items-center gap-[9px] border border-acc-line bg-acc-dim px-3 py-2.5 text-acc">
            <Icon name="layers" size={14} />
            <span className="text-[11px] tracking-[0.04em]">
              Fields below are preset by the {t.name} template.
            </span>
          </div>
          {error && (
            <div className="mb-3 flex items-center gap-2 border border-danger bg-bg px-3 py-2 text-[11px] text-danger" role="alert">
              <Icon name="warn" size={12} /> {error}
            </div>
          )}
          <FormField
            label="Instance label (optional)"
            value={vals.__label || ""}
            onChange={(v) => setVals((s) => ({ ...s, __label: v }))}
            placeholder="e.g. Production"
          />
          <EnvironmentPicker value={env} onChange={setEnv} />
          <ExpirationPicker value={expiresAt} onChange={setExpiresAt} />
          {t.fields.map((f) => {
            const fieldAssist = credentialAssistFor(t.id, f.label);
            return (
            <Fragment key={f.label}>
              <FormField
                label={f.label}
                secret={f.secret}
                area={f.area}
                hint={credentialFieldHint(t.id, f.label, f.fieldType, f.secret)}
                value={vals[f.label] || ""}
                onChange={(v) => setVals((s) => ({ ...s, [f.label]: v }))}
                placeholder={credentialPlaceholder(f.label, f.fieldType, f.secret, f.area)}
              />
              {fieldAssist && (
                <CredentialAutofillPanel
                  assist={fieldAssist}
                  busy={assist.busy && assist.field === f.label}
                  message={assist.field === f.label ? assist.message : null}
                  note={assist.field === f.label ? assist.note : null}
                  error={assist.field === f.label ? assist.error : null}
                  disabled={!vals[f.label]?.trim()}
                  onFetch={() => runAssist(f.label)}
                />
              )}
            </Fragment>
            );
          })}
          {t.totp && (
            <div className="mt-1.5">
              <FormField
                label="Authenticator secret (TOTP)"
                secret
                totpQr
                value={vals.__totp || ""}
                onChange={(v) => setVals((s) => ({ ...s, __totp: v }))}
                placeholder="paste QR image, otpauth URI, or setup key"
              />
            </div>
          )}
        </div>
      </Modal>
    );
  }

  return (
    <Modal
      title="Add a service"
      sub={project ? `To ${project.name} · pick a template` : "Pick a template"}
      onClose={onClose}
      wide
    >
      <div className="px-4 pt-4 pb-[22px] sm:px-[22px]">
        {error && (
          <div className="mb-3 flex items-center gap-2 border border-danger bg-bg px-3 py-2 text-[11px] text-danger" role="alert">
            <Icon name="warn" size={12} /> {error}
          </div>
        )}
        <div className="mb-[18px] flex items-center gap-[9px] border border-line-2 bg-bg px-3 py-[9px]">
          <span className="text-txt-3">
            <Icon name="search" size={14} />
          </span>
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="SEARCH SERVICES…"
            className="flex-1 border-none bg-transparent font-mono text-[11px] tracking-[0.08em] text-txt outline-none"
          />
        </div>
        {loading && <Lbl className="block py-4 text-txt-3">LOADING TEMPLATES…</Lbl>}
        {!loading && Object.keys(groups).length === 0 && (
          <Lbl className="block py-4 text-txt-3">NO TEMPLATES FOUND</Lbl>
        )}
        {Object.keys(groups).map((g) => (
          <div key={g} className="mb-[18px]">
            <Lbl className="mb-2.5 text-txt-4">{g}</Lbl>
            <div className="grid grid-cols-1 gap-px bg-line min-[430px]:grid-cols-2 sm:[grid-template-columns:repeat(auto-fill,minmax(150px,1fr))]">
              {groups[g].map((t) => (
                <button
                  key={t.id}
                  onClick={() => {
                    setSel(t);
                    setVals({});
                    setEnv("all");
                    setExpiresAt("");
                  }}
                  className="flex items-center gap-[11px] border-none bg-bg-1 px-3.5 py-[13px] text-left transition-colors duration-100 hover:bg-bg-3"
                >
                  <ServiceMark tpl={t} size={30} />
                  <div className="min-w-0">
                    <div className="overflow-hidden text-[12px] text-ellipsis whitespace-nowrap text-txt">
                      {t.name}
                    </div>
                    <Lbl className="mt-[3px] text-txt-4">
                      {t.fields.length} FLD{t.totp ? " · 2FA" : ""}
                    </Lbl>
                  </div>
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
    </Modal>
  );
}

/* ---------- edit service ---------- */

export function EditService({
  service,
  onClose,
  onSave,
  revealSecretsByDefault,
}: {
  service: Service;
  onClose: () => void;
  onSave: (values: ServiceDraft) => void | Promise<void>;
  revealSecretsByDefault: boolean;
}) {
  const [label, setLabel] = useState(service.label);
  const [env, setEnv] = useState<Environment>(service.env);
  const [expiresAt, setExpiresAt] = useState(service.expiresAt ?? "");
  const [vals, setVals] = useState<Record<string, string>>(() => {
    const initial: Record<string, string> = {};
    service.fields.forEach((field) => {
      initial[field.label] = field.secret ? "" : field.value ?? "";
    });
    return initial;
  });
  const [totpSecret, setTotpSecret] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [loadingSecrets, setLoadingSecrets] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [assist, setAssist] = useState<AssistState>(EMPTY_ASSIST);

  useEffect(() => {
    let alive = true;
    const secretFields = service.fields.filter((field) => field.secret && field.hasValue);
    if (!secretFields.length) return;

    setLoadingSecrets(true);
    void Promise.allSettled(
      secretFields.map(async (field) => {
        const secret = await revealField(field.id);
        return [field.label, secret.value] as const;
      }),
    )
      .then((results) => {
        if (!alive) return;
        const failed = results.some((result) => result.status === "rejected");
        setVals((current) => {
          const next = { ...current };
          for (const result of results) {
            if (result.status === "fulfilled") {
              const [label, value] = result.value;
              next[label] = value;
            }
          }
          return next;
        });
        if (failed) setError("Some encrypted values could not be loaded for editing.");
      })
      .finally(() => {
        if (alive) setLoadingSecrets(false);
      });

    return () => {
      alive = false;
    };
  }, [service.id, service.fields]);

  const submit = async () => {
    if (submitting) return;
    const fields = service.fields
      .filter((field) => !field.secret || vals[field.label])
      .map((field) => ({ label: field.label, value: vals[field.label] ?? "" }));
    try {
      setSubmitting(true);
      setError(null);
      await onSave({
        label: label.trim(),
        env,
        expiresAt,
        totpSecret: totpSecret.trim() || undefined,
        fields,
      });
    } catch (err) {
      setError((err as AppError)?.message ?? "Couldn't save the service.");
    } finally {
      setSubmitting(false);
    }
  };

  const runAssist = async (fieldLabel: string) => {
    if (assist.busy) return;
    setAssist({ field: fieldLabel, busy: true, message: null, note: null, error: null });
    try {
      const result = await runCredentialAutofill(service.templateId, fieldLabel, vals[fieldLabel] || "");
      setLabel((current) => (current.trim() || !result.label ? current : result.label));
      setVals((current) => applyAutofillValues(current, result));
      setAssist({
        field: fieldLabel,
        busy: false,
        message: result.message,
        note: result.note ?? null,
        error: null,
      });
    } catch (err) {
      setAssist({
        field: fieldLabel,
        busy: false,
        message: null,
        note: null,
        error: (err as Error)?.message ?? "Could not fetch token metadata.",
      });
    }
  };

  return (
    <Modal
      title="Edit service"
      sub={service.title}
      onClose={onClose}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose} disabled={submitting}>
            Cancel
          </Btn>
          <Btn variant="primary" onClick={() => void submit()} disabled={submitting}>
            <Icon name="check" size={14} /> {submitting ? "Saving…" : "Save changes"}
          </Btn>
        </>
      }
    >
      <div className="px-4 py-5 sm:px-[22px]">
        {error && (
          <div className="mb-3 flex items-center gap-2 border border-danger bg-bg px-3 py-2 text-[11px] text-danger" role="alert">
            <Icon name="warn" size={12} /> {error}
          </div>
        )}
        {loadingSecrets && (
          <div className="mb-3 flex items-center gap-2 border border-line-2 bg-bg px-3 py-2 text-[11px] text-txt-3" role="status">
            <Icon name="refresh" size={12} /> Loading encrypted values…
          </div>
        )}
        <FormField
          label="Instance label"
          value={label}
          onChange={setLabel}
          placeholder="e.g. Production"
          autoFocus
        />
        <EnvironmentPicker value={env} onChange={setEnv} />
        <ExpirationPicker value={expiresAt} onChange={setExpiresAt} />
        {service.fields.map((field) => {
          const fieldAssist = credentialAssistFor(service.templateId, field.label);
          return (
          <Fragment key={field.id}>
            <FormField
              label={field.label}
              secret={field.secret}
              area={field.area}
              hint={credentialFieldHint(service.templateId, field.label, field.fieldType, field.secret)}
              initiallyRevealed={field.secret ? revealSecretsByDefault : false}
              value={vals[field.label] || ""}
              onChange={(value) => setVals((s) => ({ ...s, [field.label]: value }))}
              placeholder={credentialPlaceholder(
                field.label,
                field.fieldType,
                field.secret,
                field.area,
                field.hasValue,
              )}
            />
            {fieldAssist && (
              <CredentialAutofillPanel
                assist={fieldAssist}
                busy={assist.busy && assist.field === field.label}
                message={assist.field === field.label ? assist.message : null}
                note={assist.field === field.label ? assist.note : null}
                error={assist.field === field.label ? assist.error : null}
                disabled={!vals[field.label]?.trim()}
                onFetch={() => runAssist(field.label)}
              />
            )}
          </Fragment>
          );
        })}
        {service.totp && (
          <FormField
            label="Authenticator secret (TOTP)"
            secret
            totpQr
            initiallyRevealed={revealSecretsByDefault}
            value={totpSecret}
            onChange={setTotpSecret}
            placeholder="paste QR image, otpauth URI, or leave unchanged"
          />
        )}
      </div>
    </Modal>
  );
}

function CredentialAutofillPanel({
  assist,
  busy,
  message,
  note,
  error,
  disabled,
  onFetch,
}: {
  assist: NonNullable<ReturnType<typeof credentialAssistFor>>;
  busy: boolean;
  message: string | null;
  note: string | null;
  error: string | null;
  disabled: boolean;
  onFetch: () => void | Promise<void>;
}) {
  return (
    <div className="mb-3.5 grid gap-2 border border-line-2 bg-bg px-3 py-2.5 sm:grid-cols-[auto_1fr] sm:items-start">
      <Btn type="button" variant="ghost" onClick={() => void onFetch()} disabled={busy || disabled}>
        <Icon name="refresh" size={13} /> {busy ? "Fetching..." : assist.buttonLabel}
      </Btn>
      <div className="min-w-0 font-mono text-[10px] uppercase tracking-[0.08em]">
        {error ? (
          <span className="text-danger">{error}</span>
        ) : message ? (
          <span className="text-acc">{message}</span>
        ) : (
          <span className="text-txt-4">{assist.idleText}</span>
        )}
        {note && <div className="mt-1 leading-[1.5] text-txt-4">{note}</div>}
      </div>
    </div>
  );
}

/* ---------- create project ---------- */

const PCOLORS = [
  "#5b9dff",
  "#c8ff2e",
  "#ff8a3d",
  "#b98cff",
  "#34e29a",
  "#ff5a52",
  "#60a5fa",
  "#f6821f",
];

function projectMono(name: string) {
  return (
    name
      .trim()
      .split(/\s+/)
      .map((w) => w[0])
      .join("")
      .slice(0, 2) || "NP"
  ).toUpperCase();
}

function ProjectFields({
  name,
  sub,
  color,
  error,
  autoFocus,
  onName,
  onSub,
  onColor,
}: {
  name: string;
  sub: string;
  color: string;
  error?: string | null;
  autoFocus?: boolean;
  onName: (value: string) => void;
  onSub: (value: string) => void;
  onColor: (value: string) => void;
}) {
  const mono = projectMono(name);

  return (
    <div className="flex flex-col gap-5 p-4 sm:flex-row sm:p-[22px]">
      <div className="flex-none text-center">
        <Lbl className="mb-2.5">Logo</Lbl>
        <div className="relative size-24">
          <div
            className="absolute inset-0 grid place-items-center font-display text-[30px] font-bold text-acc-ink"
            style={{ background: color }}
          >
            {mono}
          </div>
        </div>
        <Lbl className="mt-2.5 leading-[1.6] text-txt-4">MONOGRAM</Lbl>
      </div>

      <div className="flex-1">
        {error && (
          <div className="mb-3 flex items-center gap-2 border border-danger bg-bg px-3 py-2 text-[11px] text-danger" role="alert">
            <Icon name="warn" size={12} /> {error}
          </div>
        )}
        <FormField
          label="Project name"
          value={name}
          onChange={onName}
          placeholder="e.g. Nimbus"
          autoFocus={autoFocus}
        />
        <FormField
          label="Description"
          value={sub}
          onChange={onSub}
          placeholder="e.g. SaaS platform"
        />
        <Lbl className="mt-[18px] mb-2.5">Accent</Lbl>
        <div className="flex flex-wrap gap-2">
          {PCOLORS.map((c) => (
            <button
              key={c}
              type="button"
              onClick={() => onColor(c)}
              className={`size-[30px] border ${color === c ? "border-2 border-txt" : "border-line-2"}`}
              style={{ background: c }}
              aria-label={`Use ${c} accent`}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

export function CreateProject({
  onClose,
  onCreate,
}: {
  onClose: () => void;
  onCreate?: (input: CreateProjectInput) => void | Promise<void>;
}) {
  const [name, setName] = useState("");
  const [sub, setSub] = useState("");
  const [color, setColor] = useState(PCOLORS[0]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <Modal
      title="New project"
      sub="A folder for one project's secrets"
      onClose={onClose}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose} disabled={submitting}>
            Cancel
          </Btn>
          <Btn
            variant="primary"
            disabled={!name.trim() || submitting}
            onClick={() => {
              void (async () => {
                try {
                  setSubmitting(true);
                  setError(null);
                  await onCreate?.({
                    name: name.trim() || "Untitled",
                    sub: sub.trim() || "Project",
                    color,
                  });
                } catch (err) {
                  setError((err as AppError)?.message ?? "Couldn't create the project.");
                } finally {
                  setSubmitting(false);
                }
              })();
            }}
          >
            <Icon name="check" size={14} /> {submitting ? "Creating…" : "Create project"}
          </Btn>
        </>
      }
    >
      <ProjectFields
        name={name}
        sub={sub}
        color={color}
        error={error}
        autoFocus
        onName={setName}
        onSub={setSub}
        onColor={setColor}
      />
    </Modal>
  );
}

export function EditProject({
  project,
  onClose,
  onSave,
}: {
  project: Project;
  onClose: () => void;
  onSave?: (input: UpdateProjectInput) => void | Promise<void>;
}) {
  const [name, setName] = useState(project.name);
  const [sub, setSub] = useState(project.sub);
  const [color, setColor] = useState(project.color || PCOLORS[0]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <Modal
      title="Edit project"
      sub="Project name, description, and accent"
      onClose={onClose}
      footer={
        <>
          <Btn variant="ghost" onClick={onClose} disabled={submitting}>
            Cancel
          </Btn>
          <Btn
            variant="primary"
            disabled={!name.trim() || submitting}
            onClick={() => {
              void (async () => {
                try {
                  setSubmitting(true);
                  setError(null);
                  await onSave?.({
                    projectId: project.id,
                    name: name.trim() || "Untitled",
                    sub: sub.trim() || "Project",
                    color,
                  });
                } catch (err) {
                  setError((err as AppError)?.message ?? "Couldn't update the project.");
                } finally {
                  setSubmitting(false);
                }
              })();
            }}
          >
            <Icon name="check" size={14} /> {submitting ? "Saving…" : "Save changes"}
          </Btn>
        </>
      }
    >
      <ProjectFields
        name={name}
        sub={sub}
        color={color}
        error={error}
        autoFocus
        onName={setName}
        onSub={setSub}
        onColor={setColor}
      />
    </Modal>
  );
}

/* ---------- form field ---------- */

function FormField({
  label,
  value,
  onChange,
  placeholder,
  hint,
  secret,
  area,
  autoFocus,
  initiallyRevealed = false,
  totpQr,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  hint?: string;
  secret?: boolean;
  area?: boolean;
  autoFocus?: boolean;
  initiallyRevealed?: boolean;
  totpQr?: boolean;
}) {
  const [show, setShow] = useState(initiallyRevealed);
  const [qrStatus, setQrStatus] = useState<"idle" | "reading" | "ok" | "error">("idle");
  const [qrMessage, setQrMessage] = useState<string | null>(null);

  useEffect(() => {
    if (secret) setShow(initiallyRevealed);
  }, [initiallyRevealed, secret]);

  const applyTotpSecret = (result: ReturnType<typeof extractTotpSecret>) => {
    onChange(result.secret);
    setShow(true);
    setQrStatus("ok");
    const source = [result.issuer, result.account].filter(Boolean).join(" · ");
    setQrMessage(source ? `QR imported · ${source}` : "QR imported");
  };

  const handlePaste = async (
    event: ReactClipboardEvent<HTMLInputElement | HTMLTextAreaElement>,
  ) => {
    if (!totpQr) return;

    const text = event.clipboardData.getData("text/plain").trim();
    if (text.toLowerCase().startsWith("otpauth://")) {
      event.preventDefault();
      try {
        applyTotpSecret(extractTotpSecret(text));
      } catch (err) {
        setQrStatus("error");
        setQrMessage((err as Error)?.message ?? "Could not read TOTP setup code.");
      }
      return;
    }

    const imageItem = Array.from(event.clipboardData.items).find((item) =>
      item.type.startsWith("image/"),
    );
    const image = imageItem?.getAsFile();
    if (!image) return;

    event.preventDefault();
    setQrStatus("reading");
    setQrMessage("Reading QR");
    try {
      applyTotpSecret(await decodeTotpQrImage(image));
    } catch (err) {
      setQrStatus("error");
      setQrMessage((err as Error)?.message ?? "Could not read QR image.");
    }
  };

  return (
    <div className="mb-3.5">
      <Lbl className="mb-1.5 flex flex-wrap items-center gap-2">
        <span>{label}</span>
        {hint && <span className="text-acc">{hint}</span>}
      </Lbl>
      <div className="flex min-h-[44px] items-center gap-2 border border-line-2 bg-bg">
        {area ? (
          <textarea
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onPaste={handlePaste}
            placeholder={placeholder}
            rows={3}
            className="flex-1 resize-y border-none bg-transparent px-3 py-2.5 font-mono text-[12px] leading-[1.45] tracking-normal text-txt outline-none"
          />
        ) : (
          <input
            type={secret && !show ? "password" : "text"}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onPaste={handlePaste}
            placeholder={placeholder}
            autoFocus={autoFocus}
            className="min-w-0 flex-1 border-none bg-transparent px-3 py-2.5 font-mono text-[12px] leading-[1.45] tracking-normal text-txt outline-none"
          />
        )}
        {secret && (
          <button
            type="button"
            onClick={() => setShow((s) => !s)}
            className="flex border-none bg-transparent px-3 text-txt-3"
          >
            <Icon name={show ? "eyeoff" : "eye"} size={14} />
          </button>
        )}
      </div>
      {totpQr && (
        <div
          className={
            "mt-1.5 flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-[0.08em] " +
            (qrStatus === "ok"
              ? "text-acc"
              : qrStatus === "error"
                ? "text-danger"
                : "text-txt-4")
          }
        >
          <Icon name="qr" size={11} />
          {qrMessage ?? "Paste QR image or otpauth URI"}
        </div>
      )}
    </div>
  );
}
