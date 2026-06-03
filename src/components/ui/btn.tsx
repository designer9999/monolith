/**
 * Brutalist button — the MONOLITH adaptation of the shadcn button.
 * Sharp corners, mono uppercase, hairline borders. Variants extend the
 * design system as the project grows.
 */

import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "@radix-ui/react-slot";

import { cn } from "@/lib/utils";

const btnVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 border font-mono uppercase tracking-[0.1em] whitespace-nowrap transition-[background,border-color,color,transform] duration-100 outline-none select-none active:translate-y-px disabled:pointer-events-none disabled:opacity-40 [&_svg]:pointer-events-none [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: "bg-bg-2 border-line-2 text-txt hover:bg-bg-3 hover:border-line-3",
        primary:
          "bg-acc border-acc text-acc-ink font-semibold hover:shadow-[0_0_22px_var(--accent-dim)]",
        ghost: "bg-transparent border-line text-txt hover:bg-bg-2",
        danger:
          "bg-transparent border-line text-danger hover:border-danger hover:bg-danger/10",
      },
      size: {
        default: "px-3.5 py-2 text-[11px]",
        sm: "px-3 py-1.5 text-[10px]",
        icon: "size-[34px] p-2",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);

export type BtnProps = React.ComponentProps<"button"> &
  VariantProps<typeof btnVariants> & { asChild?: boolean };

export function Btn({ className, variant, size, asChild = false, ...props }: BtnProps) {
  const Comp = asChild ? Slot : "button";
  return <Comp className={cn(btnVariants({ variant, size }), className)} {...props} />;
}
