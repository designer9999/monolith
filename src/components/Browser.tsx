/**
 * MONOLITH — All Items browser (grid / list). Routes into the project view.
 * Ported 1:1 from the brutalist design; styling is Tailwind-only against the
 * mapped color tokens, building on the shared ServiceMark / Strength / Icon /
 * Chip / Lbl primitives. Per-instance project colors stay inline (dynamic).
 */

import { useMemo, useState } from "react";

import type { Item } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { expirationInfo, isExpirationAttention } from "@/lib/expiration";
import { ServiceMark, Strength, type MarkLike } from "@/lib/ui";
import { fmtDate, rel } from "@/lib/format";
import { Chip, LblText } from "@/components/ui/primitives";

type SortKey = "updated" | "name" | "project" | "strength";
type Tab = "all" | "totp" | "risk";
type Mode = "grid" | "list";

const SORTS: { key: SortKey; label: string }[] = [
  { key: "updated", label: "Updated" },
  { key: "name", label: "A–Z" },
  { key: "project", label: "Project" },
  { key: "strength", label: "Strength" },
];

function mk(item: Item): MarkLike {
  return { mono: item.mono, color: item.color, slug: item.slug, icon: item.icon };
}

const isRisk = (i: Item) => isExpirationAttention(i.expiresAt) || i.exposed || i.reused || (i.strength != null && i.strength < 45);

export interface BrowserProps {
  items: Item[];
  onOpen: (item: Item) => void;
}

export function Browser({ items, onOpen }: BrowserProps) {
  const [q, setQ] = useState("");
  const [sort, setSort] = useState<SortKey>("updated");
  const [tab, setTab] = useState<Tab>("all");
  const [mode, setMode] = useState<Mode>("grid");

  const list = useMemo(() => {
    let r = items.slice();
    if (tab === "totp") r = r.filter((i) => i.totp);
    if (tab === "risk") r = r.filter(isRisk);
    if (q.trim()) {
      const s = q.toLowerCase();
      r = r.filter(
        (i) =>
          i.title.toLowerCase().includes(s) ||
          i.projectName.toLowerCase().includes(s) ||
          i.tags.some((t) => t.toLowerCase().includes(s)),
      );
    }
    r.sort((a, b) => {
      if (sort === "name") return a.title.localeCompare(b.title);
      if (sort === "project")
        return a.projectName.localeCompare(b.projectName) || a.title.localeCompare(b.title);
      if (sort === "strength") return (b.strength ?? -1) - (a.strength ?? -1);
      return new Date(b.updated).getTime() - new Date(a.updated).getTime();
    });
    return r;
  }, [items, q, sort, tab]);

  const totpN = items.filter((i) => i.totp).length;
  const riskN = items.filter(isRisk).length;

  const tabs: [Tab, string][] = [
    ["all", "ALL"],
    ["totp", "2FA · " + totpN],
    ["risk", "RISK · " + riskN],
  ];

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* toolbar */}
      <div className="flex flex-col gap-3 border-b border-line px-4 py-4 sm:flex-row sm:items-center sm:gap-3.5 sm:px-[22px]">
        <div className="flex items-baseline gap-2.5">
          <span className="font-display text-[18px] font-semibold">All Items</span>
          <span className="font-mono text-[11px] tabular-nums text-txt-3">
            [{String(list.length).padStart(2, "0")}]
          </span>
        </div>

        {/* tabs */}
        <div className="flex overflow-x-auto border border-line-2 sm:ml-1.5">
          {tabs.map(([k, l], i) => (
            <button
              key={k}
              onClick={() => setTab(k)}
              className={`px-3 py-[7px] font-mono text-[10px] uppercase tracking-[0.1em] ${
                i ? "border-l border-line" : ""
              } ${
                tab === k
                  ? `bg-bg-3 ${k === "risk" ? "text-danger" : "text-acc"}`
                  : "text-txt-3"
              }`}
            >
              {l}
            </button>
          ))}
        </div>

        <div className="hidden flex-1 sm:block" />

        {/* search */}
        <div className="flex w-full items-center gap-[9px] border border-line-2 bg-bg px-3 py-[9px] sm:w-60 sm:py-[7px]">
          <Icon name="search" size={14} className="text-txt-3" />
          <input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="SEARCH ITEMS…"
            className="flex-1 border-none bg-transparent font-mono text-[11px] tracking-[0.08em] text-txt outline-none"
          />
          {q && (
            <button onClick={() => setQ("")} className="flex text-txt-3 hover:text-txt">
              <Icon name="x" size={12} />
            </button>
          )}
        </div>

        {/* sorts */}
        <div className="flex overflow-x-auto border border-line-2">
          {SORTS.map((s, i) => (
            <button
              key={s.key}
              onClick={() => setSort(s.key)}
              className={`px-[11px] py-[7px] font-mono text-[10px] uppercase tracking-[0.1em] ${
                i ? "border-l border-line" : ""
              } ${sort === s.key ? "bg-bg-3 text-txt" : "text-txt-3"}`}
            >
              {s.label}
            </button>
          ))}
        </div>

        {/* mode toggle */}
        <div className="flex w-fit border border-line-2">
          <button
            onClick={() => setMode("grid")}
            className={`flex size-[34px] items-center justify-center bg-transparent hover:bg-bg-2 ${
              mode === "grid" ? "text-acc" : "text-txt-3"
            }`}
          >
            <Icon name="grid" size={14} />
          </button>
          <button
            onClick={() => setMode("list")}
            className={`flex size-[34px] items-center justify-center border-l border-line bg-transparent hover:bg-bg-2 ${
              mode === "list" ? "text-acc" : "text-txt-3"
            }`}
          >
            <Icon name="list" size={14} />
          </button>
        </div>
      </div>

      {list.length === 0 ? (
        <EmptyState q={q} />
      ) : mode === "grid" ? (
        <div className="flex-1 overflow-auto bg-line p-px">
          <div className="grid grid-cols-[repeat(auto-fill,minmax(258px,1fr))] gap-px">
            {list.map((it) => (
              <Card key={it.id} item={it} onOpen={onOpen} />
            ))}
          </div>
        </div>
      ) : (
        <div className="flex-1 overflow-auto">
          <div className="min-w-[720px]">
            <ListHeader />
            {list.map((it) => (
              <Row key={it.id} item={it} onOpen={onOpen} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function Card({ item, onOpen }: { item: Item; onOpen: (item: Item) => void }) {
  const exp = expirationInfo(item.expiresAt);
  return (
    <button
      onClick={() => onOpen(item)}
      className="flex min-h-[146px] cursor-pointer flex-col gap-3.5 border-none bg-bg-1 p-4 text-left transition-colors duration-[120ms] hover:bg-bg-2"
    >
      <div className="flex items-start justify-between">
        <ServiceMark tpl={mk(item)} size={34} />
        <div className="flex items-center gap-1.5">
          {item.totp && (
            <Chip tone="accent" className="gap-1 px-1.5 py-0.5">
              <Icon name="refresh" size={9} />
              2FA
            </Chip>
          )}
          {item.exposed && (
            <span className="text-danger">
              <Icon name="warn" size={13} />
            </span>
          )}
          {exp.tone === "expired" && (
            <Chip tone="danger" className="gap-1 px-1.5 py-0.5">
              EXPIRED
            </Chip>
          )}
          {exp.tone === "soon" && (
            <Chip tone="warn" className="gap-1 px-1.5 py-0.5">
              EXPIRING
            </Chip>
          )}
          {item.fav && (
            <span className="text-acc">
              <Icon name="star" size={13} fill />
            </span>
          )}
        </div>
      </div>

      <div className="flex-1">
        <div className="mb-1.5 font-display text-[14px] font-semibold leading-[1.3]">
          {item.title}
        </div>
        <div className="flex items-center gap-[7px]">
          <span
            className="size-[7px] flex-none"
            style={{ background: item.projectColor }}
          />
          <LblText className="text-txt-3">{item.projectName}</LblText>
        </div>
      </div>

      <div className="flex items-center justify-between gap-2">
        <LblText className="text-txt-4">{rel(item.updated)} AGO</LblText>
        {item.strength != null ? (
          <Strength value={item.strength} w={44} />
        ) : (
          <span className="font-mono text-[10px] tabular-nums text-txt-3">
            {item.fieldCount} FLD
          </span>
        )}
      </div>
    </button>
  );
}

function ListHeader() {
  return (
    <div className="sticky top-0 z-[2] grid grid-cols-[42px_1fr_150px_80px_90px_80px] gap-3.5 border-b border-line-2 bg-bg-1 px-[22px] py-2.5">
      {["", "SERVICE", "PROJECT", "STRENGTH", "UPDATED", "2FA"].map((h, i) => (
        <LblText key={i} className="text-txt-4">
          {h}
        </LblText>
      ))}
    </div>
  );
}

function Row({ item, onOpen }: { item: Item; onOpen: (item: Item) => void }) {
  const exp = expirationInfo(item.expiresAt);
  return (
    <button
      onClick={() => onOpen(item)}
      className="grid w-full cursor-pointer grid-cols-[42px_1fr_150px_80px_90px_80px] items-center gap-3.5 border-none border-b border-solid border-line bg-transparent px-[22px] py-[11px] text-left transition-colors duration-100 hover:bg-bg-2"
    >
      <ServiceMark tpl={mk(item)} size={30} />
      <span className="flex min-w-0 items-center gap-[9px]">
        <span className="overflow-hidden text-ellipsis whitespace-nowrap font-display text-[13px] font-medium text-txt">
          {item.title}
        </span>
        {item.fav && (
          <span className="flex-none text-acc">
            <Icon name="star" size={11} fill />
          </span>
        )}
        {item.exposed && (
          <span className="flex-none text-danger">
            <Icon name="warn" size={11} />
          </span>
        )}
        {(exp.tone === "expired" || exp.tone === "soon") && (
          <span className={exp.tone === "expired" ? "flex-none text-danger" : "flex-none text-warn"}>
            <Icon name="clock" size={11} />
          </span>
        )}
      </span>
      <span className="flex min-w-0 items-center gap-[7px]">
        <span className="size-[7px] flex-none" style={{ background: item.projectColor }} />
        <LblText className="overflow-hidden text-ellipsis whitespace-nowrap text-txt-2">
          {item.projectName}
        </LblText>
      </span>
      <span>
        {item.strength != null ? (
          <Strength value={item.strength} w={56} />
        ) : (
          <span className="text-[10px] text-txt-3">—</span>
        )}
      </span>
      <span className="font-mono text-[11px] tabular-nums text-txt-2">{fmtDate(item.updated)}</span>
      <span>
        {item.totp ? (
          <span className="flex text-acc">
            <Icon name="refresh" size={13} />
          </span>
        ) : (
          <span className="text-[10px] text-txt-3">—</span>
        )}
      </span>
    </button>
  );
}

function EmptyState({ q }: { q: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-[18px] text-txt-3">
      <div className="grid size-16 place-items-center border border-dashed border-line-2">
        <Icon name={q ? "search" : "folder"} size={26} />
      </div>
      <LblText className="text-center leading-[1.8]">
        {q ? `NO MATCH FOR "${q.toUpperCase()}"` : "NOTHING HERE YET"}
      </LblText>
    </div>
  );
}
