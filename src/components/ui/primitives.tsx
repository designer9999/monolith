/**
 * Small MONOLITH design-system atoms, styled with Tailwind utilities + CVA
 * variants. These replace the design's `.lbl` / `.chip` / icon-button CSS so all
 * styling lives in components.
 */

import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";
import { Icon } from "@/lib/icons";

/** Uppercase mono micro-label (the design's `.lbl`). */
export function Lbl({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      className={cn("font-mono text-[10px] tracking-[0.18em] uppercase text-txt-3", className)}
      {...props}
    />
  );
}

/** Inline span variant of Lbl. */
export function LblText({ className, ...props }: React.ComponentProps<"span">) {
  return (
    <span
      className={cn("font-mono text-[10px] tracking-[0.18em] uppercase text-txt-3", className)}
      {...props}
    />
  );
}

const chipVariants = cva(
  "inline-flex items-center gap-1.5 border px-2 py-[3px] text-[10px] tracking-[0.08em] uppercase whitespace-nowrap",
  {
    variants: {
      tone: {
        default: "border-line-2 text-txt-2",
        accent: "border-acc-line text-acc",
        danger: "border-danger text-danger",
        warn: "border-warn text-warn",
        ok: "border-ok text-ok",
        info: "border-info text-info",
        current: "border-current",
      },
    },
    defaultVariants: { tone: "default" },
  },
);

export type ChipProps = React.ComponentProps<"span"> & VariantProps<typeof chipVariants>;

/** Brutalist chip / tag (the design's `.chip`). */
export function Chip({ className, tone, ...props }: ChipProps) {
  return <span className={cn(chipVariants({ tone }), className)} {...props} />;
}

/** A small square icon button with active (copied/ok) state. */
export function IconBtn({
  active,
  className,
  children,
  ...props
}: React.ComponentProps<"button"> & { active?: boolean }) {
  return (
    <button
      className={cn(
        "inline-flex size-[34px] items-center justify-center border bg-transparent p-2 transition-colors duration-100",
        active ? "border-ok text-ok" : "border-line text-txt-2 hover:text-txt",
        className,
      )}
      {...props}
    >
      {children}
    </button>
  );
}

/** Section header: small icon + label + optional right meta (the design's SectionHead). */
export function SectionHead({ icon, title, right }: { icon: string; title: string; right?: string }) {
  return (
    <div className="mb-3.5 flex items-center gap-[9px]">
      <span className="text-txt-3">
        <Icon name={icon} size={14} />
      </span>
      <LblText className="flex-1 text-txt-2">{title}</LblText>
      {right && <LblText className="text-txt-4">{right}</LblText>}
    </div>
  );
}

/** Numeric mini-stat with a label and tone. */
export function MiniStat({
  n,
  label,
  tone,
}: {
  n: number;
  label: string;
  tone?: "accent" | "danger" | "ok" | null;
}) {
  const color =
    tone === "accent" ? "text-acc" : tone === "danger" ? "text-danger" : tone === "ok" ? "text-ok" : "text-txt";
  return (
    <div>
      <div className={cn("font-display tabular-nums text-2xl font-bold leading-none", color)}>
        {String(n).padStart(2, "0")}
      </div>
      <Lbl className="mt-[5px]">{label}</Lbl>
    </div>
  );
}
