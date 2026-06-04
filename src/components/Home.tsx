/**
 * Home: projects grid + vault-health distribution + live 2FA strip + security
 * attention + recent activity. Ported 1:1 from the design's home.jsx; data is
 * real (projects/items/activity from the vault), TOTP codes from Rust.
 */

import { useState } from "react";

import type { Activity, Item, Project } from "@/lib/types";
import type { ProjectIcon as ProjectIconData } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { expirationInfo, isExpirationAttention } from "@/lib/expiration";
import { fmtDateNice, rel } from "@/lib/format";
import { cn } from "@/lib/utils";
import { ServiceMark, useCopy } from "@/lib/ui";
import { Btn } from "@/components/ui/btn";
import { Chip, Lbl, LblText, SectionHead } from "@/components/ui/primitives";
import { ProjectIcon } from "./ProjectIcon";
import { ProjectActionsMenu } from "./ProjectActionsMenu";
import { TotpChip } from "./Totp";

export function Home({
  projects,
  items,
  activity,
  onOpenProject,
  onNewProject,
  onSetIcon,
  onEditProject,
  onDeleteProject,
  onReorder,
}: {
  projects: Project[];
  items: Item[];
  activity: Activity[];
  onOpenProject: (p: Project, item?: Item) => void;
  onNewProject: () => void;
  onSetIcon: (id: string, icon: ProjectIconData | null) => void;
  onEditProject: (p: Project) => void;
  onDeleteProject: (p: Project) => void | Promise<void>;
  onReorder: (orderedIds: string[]) => void;
}) {
  const [copied, copy] = useCopy();
  const [drag, setDrag] = useState<number | null>(null);
  const [over, setOver] = useState<number | null>(null);
  const personal = projects.find((p) => p.personal);
  const regularProjects = projects.filter((p) => !p.personal);

  const codes = items.filter((i) => i.totp);
  const expiring = items.filter((i) => isExpirationAttention(i.expiresAt));
  const exposed = items.filter((i) => i.exposed);
  const reused = items.filter((i) => i.reused);
  const risk = items.filter((i) => isExpirationAttention(i.expiresAt) || i.exposed || i.reused);

  const hr = new Date().getHours();
  const greet = hr < 5 ? "LATE NIGHT" : hr < 12 ? "GOOD MORNING" : hr < 18 ? "GOOD AFTERNOON" : "GOOD EVENING";
  const kindColor: Record<string, string> = {
    copy: "var(--accent)",
    view: "var(--info)",
    add: "var(--ok)",
    edit: "var(--txt-2)",
    warn: "var(--danger)",
  };
  const hc = exposed.length ? "var(--danger)" : risk.length ? "var(--warn)" : "var(--ok)";

  const onDrop = (idx: number) => {
    if (drag == null || drag === idx) {
      setDrag(null);
      setOver(null);
      return;
    }
    const next = [...regularProjects];
    const [m] = next.splice(drag, 1);
    next.splice(idx, 0, m);
    onReorder([...(personal ? [personal.id] : []), ...next.map((p) => p.id)]);
    setDrag(null);
    setOver(null);
  };

  const total = Math.max(items.length, 1);
  const clear = Math.max(items.length - risk.length, 0);
  const dist: [string, number, string][] = [
    ["var(--ok)", clear, "CLEAR"],
    ["var(--warn)", expiring.length, "EXPIRING"],
    ["var(--danger)", exposed.length + reused.length, "ACTION"],
    ["var(--line-3)", codes.length, "2FA"],
  ];

  return (
    <div className="h-full overflow-y-auto">
      <div className="flex flex-col gap-4 border-b border-line px-4 pt-5 pb-4 sm:flex-row sm:items-end sm:justify-between sm:px-[30px] sm:pt-[26px] sm:pb-[22px]">
        <div>
          <Lbl className="mb-2">
            {greet} · {fmtDateNice(new Date().toISOString())}
          </Lbl>
          <h1 className="font-display text-[24px] font-bold tracking-normal sm:text-[30px]">
            Every project, sealed.
            <span className="ml-0.5 inline-block h-[1.1em] w-[2px] translate-y-[0.15em] animate-pulse bg-acc align-middle" />
          </h1>
        </div>
        <Btn variant="primary" className="w-full justify-center sm:w-auto" onClick={onNewProject}>
          <Icon name="plus" size={14} /> New Project
        </Btn>
      </div>

      <div className="flex flex-col gap-px bg-line p-3 sm:p-6">
        {/* hero: health + live 2FA */}
        <div className="grid grid-cols-1 gap-px bg-line lg:grid-cols-[340px_1fr]">
          <div className="bg-bg-1 px-[22px] py-5">
            <SectionHead icon="shield" title="Vault overview" right={`${items.length} SECRETS`} />
            <div className="mb-[18px] flex items-end gap-4">
              <div
                className="font-display tabular-nums text-[60px] font-bold leading-[0.8]"
                style={{ color: hc }}
              >
                {String(items.length).padStart(2, "0")}
              </div>
              <div className="pb-1.5">
                <Lbl style={{ color: hc }}>
                  {risk.length ? "ATTENTION" : "SEALED"}
                </Lbl>
                <Lbl className="mt-[5px] text-txt-4">{risk.length} EXPIRING / EXPOSED / REUSED</Lbl>
              </div>
            </div>
            <div className="mb-3 flex h-2.5 border border-line-2 bg-bg">
              {dist.map(([c, n], i) =>
                n > 0 ? (
                  <div
                    key={i}
                    className={i < dist.length - 1 ? "border-r border-bg" : ""}
                    style={{ width: `${(n / total) * 100}%`, background: c }}
                  />
                ) : null,
              )}
            </div>
            <div className="grid grid-cols-2 gap-x-3.5 gap-y-2">
              {dist.map(([c, n, l], i) => (
                <div key={i} className="flex items-center gap-2">
                  <span className="size-2 flex-none" style={{ background: c }} />
                  <LblText className="flex-1">{l}</LblText>
                  <span className="tabular-nums text-[11px] text-txt-2">{String(n).padStart(2, "0")}</span>
                </div>
              ))}
            </div>
          </div>

          <div className="min-w-0 bg-bg-1 px-[22px] py-5">
            <SectionHead icon="refresh" title="Live · Authenticator" right={`${codes.length} CODES · TAP TO COPY`} />
            <div
              className="grid grid-flow-col auto-cols-[minmax(190px,82vw)] gap-px overflow-x-auto bg-line pb-0.5 sm:auto-cols-[232px]"
              style={{ gridTemplateRows: codes.length > 1 ? "1fr 1fr" : "1fr" }}
            >
              {codes.map((it) => (
                <TotpChip key={it.id} item={it} copy={copy} copied={copied} />
              ))}
            </div>
          </div>
        </div>

        {/* projects grid */}
        <div className="bg-bg-1 px-5 pt-[18px] pb-[22px]">
          {personal && (
            <div className="mb-5">
              <div className="mb-3 flex items-center gap-[9px]">
                <span className="text-txt-3">
                  <Icon name="vault" size={14} />
                </span>
                <LblText className="flex-1 text-txt-2">Personal vault</LblText>
                <LblText className="text-txt-4">GLOBAL · NOT TIED TO A PROJECT</LblText>
              </div>
              <ProjectCard
                p={personal}
                onSetIcon={onSetIcon}
                onEditProject={onEditProject}
                onDeleteProject={onDeleteProject}
                isDrag={false}
                isOver={false}
                onOpen={() => onOpenProject(personal)}
                onDragStart={() => undefined}
                onDragEnter={() => undefined}
                onDragEnd={() => undefined}
                onDrop={() => undefined}
                draggable={false}
              />
            </div>
          )}
          <div className="mb-4 flex items-center gap-[9px]">
            <span className="text-txt-3">
              <Icon name="folder" size={14} />
            </span>
            <LblText className="flex-1 text-txt-2">Projects</LblText>
            <LblText className="hidden text-txt-4 sm:inline">DRAG TO REORDER</LblText>
          </div>
          <div className="grid grid-cols-1 gap-px bg-line sm:grid-cols-[repeat(auto-fill,minmax(300px,1fr))]">
            {regularProjects.map((p, idx) => (
              <ProjectCard
                key={p.id}
                p={p}
                onSetIcon={onSetIcon}
                onEditProject={onEditProject}
                onDeleteProject={onDeleteProject}
                isDrag={drag === idx}
                isOver={over === idx && drag !== idx}
                onOpen={() => onOpenProject(p)}
                onDragStart={() => setDrag(idx)}
                onDragEnter={() => setOver(idx)}
                onDragEnd={() => {
                  setDrag(null);
                  setOver(null);
                }}
                onDrop={() => onDrop(idx)}
              />
            ))}
            <button
              onClick={onNewProject}
              className="flex min-h-[158px] flex-col items-center justify-center gap-3 border border-dashed border-line-2 bg-bg-1 text-txt-3 transition-colors duration-100 hover:border-acc-line hover:text-acc"
            >
              <Icon name="plus" size={22} />
              <Lbl>NEW PROJECT</Lbl>
            </button>
          </div>
        </div>

        {/* security + activity */}
        <div className="grid grid-cols-1 gap-px bg-line lg:grid-cols-[1.1fr_1fr]">
          <div className="bg-bg-1 px-5 py-[18px]">
            <SectionHead icon="shield" title="Security · attention" right={`${risk.length} ISSUES`} />
            {risk.length === 0 && <Lbl className="py-2 text-ok">ALL SECRETS HEALTHY</Lbl>}
            {risk.slice(0, 4).map((it) => (
              <button
                key={it.id}
                onClick={() => {
                  const p = projects.find((x) => x.id === it.projectId);
                  if (p) onOpenProject(p, it);
                }}
                className="flex w-full items-center gap-3 border-b border-line py-[11px] text-left"
              >
                <ServiceMark tpl={{ mono: it.mono, color: it.color, slug: it.slug, icon: it.icon }} size={26} />
                <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[12px] text-txt">
                  {it.projectName} · {it.title}
                </span>
                <Chip tone={it.exposed || expirationInfo(it.expiresAt).tone === "expired" ? "danger" : "warn"}>
                  {it.exposed
                    ? "EXPOSED"
                    : it.reused
                      ? "REUSED"
                      : expirationInfo(it.expiresAt).tone === "expired"
                        ? "EXPIRED"
                        : "EXPIRING"}
                </Chip>
              </button>
            ))}
          </div>
          <div className="bg-bg-1 px-5 py-[18px]">
            <SectionHead icon="clock" title="Recent activity" right="LIVE" />
            {activity.slice(0, 5).map((a) => (
              <div key={`${a.time}-${a.target}`} className="flex items-center gap-3 border-b border-line py-[9px]">
                <span className="size-1.5 flex-none" style={{ background: kindColor[a.kind] || "var(--txt-3)" }} />
                <LblText className="w-[52px]" style={{ color: kindColor[a.kind] || "var(--txt-3)" }}>
                  {a.action}
                </LblText>
                <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[12px] text-txt-2">
                  {a.target}
                </span>
                <span className="tabular-nums text-[10px] text-txt-3">{rel(a.time)}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function ProjectCard({
  p,
  onSetIcon,
  onEditProject,
  onDeleteProject,
  isDrag,
  isOver,
  onOpen,
  onDragStart,
  onDragEnter,
  onDragEnd,
  onDrop,
  draggable = true,
}: {
  p: Project;
  onSetIcon: (id: string, icon: ProjectIconData | null) => void;
  onEditProject: (p: Project) => void;
  onDeleteProject: (p: Project) => void | Promise<void>;
  isDrag: boolean;
  isOver: boolean;
  onOpen: () => void;
  onDragStart: () => void;
  onDragEnter: () => void;
  onDragEnd: () => void;
  onDrop: () => void;
  draggable?: boolean;
}) {
  return (
    <div
      draggable={draggable}
      onDragStart={(e) => {
        if (!draggable) return;
        e.dataTransfer.effectAllowed = "move";
        onDragStart();
      }}
      onDragEnter={onDragEnter}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        onDrop();
      }}
      onDragEnd={onDragEnd}
      onClick={onOpen}
      className={cn(
        "relative cursor-pointer bg-bg-1 p-4 transition-[background,box-shadow] duration-100 sm:p-[18px]",
        isDrag ? "opacity-35" : "hover:bg-bg-2",
        isOver && "shadow-[inset_0_0_0_1px_var(--accent)]",
      )}
    >
      <div className="mb-4 flex items-start gap-3">
        <ProjectIcon project={p} size={42} onSetIcon={onSetIcon} />
        <div className="min-w-0 flex-1">
          <div className="font-display overflow-hidden text-ellipsis whitespace-nowrap text-[15px] font-semibold sm:text-[16px]">
            {p.name}
          </div>
          <Lbl className="mt-1">{p.sub}</Lbl>
        </div>
        <div className="flex items-center gap-1">
          {draggable && (
            <span className="hidden cursor-grab text-txt-4 sm:inline-flex" title="Drag to reorder">
              <Icon name="drag" size={16} />
            </span>
          )}
          <ProjectActionsMenu
            project={p}
            onOpen={() => onOpen()}
            onEditProject={onEditProject}
            onDeleteProject={onDeleteProject}
            onSetIcon={onSetIcon}
          />
        </div>
      </div>
      <div className="mb-3.5 flex w-fit gap-px bg-line">
        {p.marks.map((m, i) => (
          <ServiceMark key={i} tpl={m} size={26} />
        ))}
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-line pt-3">
        <LblText>{String(p.count).padStart(2, "0")} SERVICES</LblText>
        <span className="flex items-center gap-2.5">
          {p.totpCount > 0 && (
            <Chip tone="accent">
              <Icon name="refresh" size={10} />
              {p.totpCount} 2FA
            </Chip>
          )}
          <LblText className="text-txt-4">{rel(p.updated)}</LblText>
        </span>
      </div>
    </div>
  );
}
