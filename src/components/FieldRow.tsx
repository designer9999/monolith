/**
 * A single secret/credential field row with reveal + copy.
 * The plaintext is fetched on demand from the Rust core (one value at a time)
 * via `revealField` — it's never shipped with the service list. Visuals ported
 * 1:1 from the design's `FieldRow`.
 */

import { useEffect, useRef, useState, type RefObject } from "react";

import { revealField } from "@/lib/tauri";
import type { FieldView } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { maskOf } from "@/lib/format";
import { cn } from "@/lib/utils";
import { Chip, IconBtn, LblText } from "@/components/ui/primitives";

export function FieldRow({
  field,
  idx,
  copy,
  copied,
  revealByDefault,
  onSave,
}: {
  field: FieldView;
  idx: number;
  copy: (t: string, k?: string) => void;
  copied: string | true | null;
  revealByDefault: boolean;
  onSave?: (field: FieldView, value: string) => void | Promise<void>;
}) {
  // Plaintext is only held in state while the field is actually being shown.
  // JS strings cannot be zeroized, so the goal here is a short lifetime.
  const [revealed, setRevealed] = useState<string | null>(field.secret ? null : field.value ?? "");
  const [show, setShow] = useState(!field.secret);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const editRef = useRef<HTMLInputElement | HTMLTextAreaElement | null>(null);
  const originalDraftRef = useRef("");
  const savingRef = useRef(false);
  const closingRef = useRef(false);

  useEffect(() => {
    setShow(!field.secret);
    setRevealed(field.secret ? null : field.value ?? "");
    setEditing(false);
    setDraft("");
    setError(null);
    originalDraftRef.current = "";
    closingRef.current = false;
  }, [field.id, field.secret, field.value]);

  useEffect(() => {
    if (!editing) return;
    const input = editRef.current;
    if (!input) return;
    const frame = window.requestAnimationFrame(() => {
      const end = input.value.length;
      input.focus();
      input.setSelectionRange(end, end);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [editing]);

  useEffect(() => {
    if (!field.secret || !field.hasValue) return;
    if (!revealByDefault) {
      setShow(false);
      setRevealed(null);
      return;
    }
    let alive = true;
    void revealField(field.id)
      .then((secret) => {
        if (!alive) return;
        setRevealed(secret.value);
        setShow(true);
      })
      .catch(() => {
        if (alive) setError("Reveal failed");
      });
    return () => {
      alive = false;
    };
  }, [field.id, field.hasValue, field.secret, revealByDefault]);

  const empty = !field.hasValue;
  const visible = !field.secret || show;

  const revealCurrent = async () => {
    if (!field.secret) return field.value ?? "";
    if (!field.hasValue) return "";
    if (revealed !== null) return revealed;
    const secret = (await revealField(field.id)).value;
    setRevealed(secret);
    setShow(true);
    return secret;
  };

  const onToggle = async () => {
    if (show) {
      setShow(false);
      if (field.secret) setRevealed(null); // wipe on hide
    } else {
      try {
        setError(null);
        if (field.secret) await revealCurrent();
        setShow(true);
      } catch {
        setError("Reveal failed");
      }
    }
  };

  const beginEdit = async () => {
    if (!onSave || saving) return;
    try {
      setError(null);
      const current = await revealCurrent();
      originalDraftRef.current = current;
      closingRef.current = false;
      setDraft(current);
      setEditing(true);
    } catch {
      setError("Reveal failed");
    }
  };

  const cancelEdit = () => {
    closingRef.current = true;
    setEditing(false);
    setDraft("");
    setError(null);
  };

  const saveEdit = async () => {
    if (!onSave || savingRef.current || closingRef.current) return;
    const next = draft;
    if (next === originalDraftRef.current) {
      closingRef.current = true;
      setEditing(false);
      setDraft("");
      setError(null);
      return;
    }
    if (field.secret && draft.length === 0) {
      setError("Enter a value to update this secret.");
      window.requestAnimationFrame(() => {
        const input = editRef.current;
        if (!input) return;
        input.focus();
        const end = input.value.length;
        input.setSelectionRange(end, end);
      });
      return;
    }
    try {
      savingRef.current = true;
      setSaving(true);
      setError(null);
      await onSave(field, next);
      originalDraftRef.current = next;
      setRevealed(next);
      if (field.secret) setShow(true);
      closingRef.current = true;
      setEditing(false);
    } catch {
      setError("Save failed");
      window.requestAnimationFrame(() => {
        const input = editRef.current;
        if (!input) return;
        input.focus();
        const end = input.value.length;
        input.setSelectionRange(end, end);
      });
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  };

  // Copy fetches a fresh value and does NOT persist it in component state.
  const onCopy = async () => {
    try {
      setError(null);
      const v = field.secret ? (await revealField(field.id)).value : (field.value ?? "");
      copy(v, `f${idx}`);
    } catch {
      setError("Copy failed");
    }
  };

  const display = empty
    ? "not set"
    : visible
      ? revealed ?? "••••••"
      : maskOf(field.value ?? "x".repeat(12));

  return (
    <div
      className={cn(
        "group -ml-[11px] grid grid-cols-[1fr_auto] items-center gap-2.5 border-b border-l-2 border-line py-[11px] pr-0 pl-[11px]",
        field.danger ? "border-l-warn" : "border-l-transparent",
      )}
    >
      <div className="min-w-0">
        <div className="mb-[5px] flex items-center gap-2">
          <LblText>{field.label}</LblText>
          {field.fieldType !== "text" && (
            <LblText className="text-[9px] text-txt-4">{field.fieldType.replace("_", " ")}</LblText>
          )}
          {field.danger && (
            <Chip tone="default" className="gap-1 px-[5px] py-px text-[9px]">
              <Icon name="key" size={9} /> SENSITIVE
            </Chip>
          )}
        </div>
        {editing ? (
          <div className="flex min-w-0 items-center gap-2">
            {field.area ? (
              <textarea
                ref={editRef as RefObject<HTMLTextAreaElement>}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onBlur={() => void saveEdit()}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    e.preventDefault();
                    cancelEdit();
                  }
                  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    void saveEdit();
                  }
                }}
                rows={Math.max(6, Math.min(18, draft.split("\n").length))}
                disabled={saving}
                spellCheck={false}
                aria-label={`Edit ${field.label}`}
                className="block max-h-[28rem] min-h-[9rem] w-full resize-y overflow-y-auto border border-line-2 bg-bg px-3 py-2 font-mono text-[13px] leading-[1.5] tracking-normal text-txt outline-none focus:border-acc disabled:opacity-60"
              />
            ) : (
              <input
                ref={editRef as RefObject<HTMLInputElement>}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                onBlur={() => void saveEdit()}
                onKeyDown={(e) => {
                  if (e.key === "Escape") {
                    e.preventDefault();
                    cancelEdit();
                  }
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void saveEdit();
                  }
                }}
                disabled={saving}
                spellCheck={false}
                aria-label={`Edit ${field.label}`}
                className="block min-w-0 flex-1 border-none bg-transparent p-0 font-mono text-[13px] leading-[1.45] tracking-normal text-txt outline-none disabled:opacity-60"
              />
            )}
            {onSave && <span aria-hidden className="size-6 flex-none" />}
          </div>
        ) : (
          <div className="group/field flex min-w-0 items-center gap-2">
            <FieldDisplay value={display} area={field.area} empty={empty} visible={visible} />
            {onSave && (
              <button
                type="button"
                title="Edit field"
                onClick={() => void beginEdit()}
                className="grid size-6 flex-none place-items-center border border-line text-txt-3 opacity-0 transition-opacity hover:border-acc-line hover:text-acc group-hover:opacity-100 group-focus-within:opacity-100"
              >
                <Icon name="pencil" size={11} />
              </button>
            )}
          </div>
        )}
        {error && <div className="mt-1 text-[10px] text-danger">{error}</div>}
      </div>
      <div className="flex gap-1.5 self-start">
        {field.secret && !empty && (
          <IconBtn onClick={onToggle} title={show ? "Hide" : "Reveal"}>
            <Icon name={show ? "eyeoff" : "eye"} size={13} />
          </IconBtn>
        )}
        <CopyBtnAsync onCopy={onCopy} active={copied === `f${idx}`} disabled={empty} />
      </div>
    </div>
  );
}

function FieldDisplay({
  value,
  area,
  empty,
  visible,
}: {
  value: string;
  area: boolean;
  empty: boolean;
  visible: boolean;
}) {
  const base = cn(
    "min-w-0 overflow-hidden font-mono text-[13px] leading-[1.45] text-ellipsis",
    empty ? "text-txt-4" : visible ? "text-txt" : "text-txt-2",
    visible && !empty ? "tracking-normal" : "tracking-[0.12em]",
  );

  if (!area || !visible || empty) {
    return <div className={cn(base, "overflow-y-hidden whitespace-nowrap")}>{value}</div>;
  }

  const blocks = value
    .split(/\n\s*-{3,}\s*\n/g)
    .map((block) => block.trim())
    .filter(Boolean);

  if (blocks.length <= 1) {
    return (
      <pre
        className={cn(
          base,
          "max-h-[32rem] whitespace-pre-wrap break-words border-l border-line-2 pl-3 pr-2 font-mono overflow-y-auto select-text",
        )}
      >
        {value}
      </pre>
    );
  }

  return (
    <div className={cn(base, "max-h-[32rem] space-y-3 overflow-y-auto pr-2 select-text")}>
      {blocks.map((block, index) => (
        <pre
          key={`${index}-${block.slice(0, 24)}`}
          className="whitespace-pre-wrap break-words border-l border-line-2 bg-bg/40 px-3 py-2 font-mono text-[13px] leading-[1.5]"
        >
          {block}
        </pre>
      ))}
    </div>
  );
}

/** Copy button variant whose click resolves the (possibly async) value first. */
function CopyBtnAsync({ onCopy, active, disabled }: { onCopy: () => void; active: boolean; disabled?: boolean }) {
  return (
    <IconBtn
      active={active}
      disabled={disabled}
      title="Copy"
      className={cn(disabled && "cursor-not-allowed opacity-40")}
      onClick={(e) => {
        e.stopPropagation();
        if (!disabled) void onCopy();
      }}
    >
      <Icon name={active ? "check" : "copy"} size={13} />
    </IconBtn>
  );
}
