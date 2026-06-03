/**
 * Frameless window chrome: the draggable titlebar (real Tauri window controls)
 * and the left sidebar (nav + project list + storage). Ported 1:1 from the
 * design's chrome.jsx; window buttons wired to the Tauri window API.
 */

import type { KeyboardEvent } from "react";
import type { Project, Storage } from "@/lib/types";
import type { ProjectIcon as ProjectIconData } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { Lbl, LblText } from "@/components/ui/primitives";
import { winClose, winMinimize, winToggleMaximize } from "@/lib/tauri";
import { ProjectIcon } from "./ProjectIcon";
import { ProjectActionsMenu } from "./ProjectActionsMenu";

export type View = "home" | "project" | "browse" | "settings";

export function TitleBar({ locked, onToggleLock }: { locked: boolean; onToggleLock: () => void }) {
  return (
    <div
      className="relative z-30 hidden h-9 items-center gap-3.5 border-b border-line bg-bg pl-3.5 md:flex"
      data-tauri-drag-region
    >
      <div className="flex items-center gap-[9px]">
        <div className="relative size-4 bg-acc shadow-[0_0_0_1px_var(--accent-line),0_0_18px_var(--accent-dim)]">
          <div className="absolute inset-1 bg-acc-ink" />
        </div>
        <div className="pl-0.5 font-display text-[13px] font-bold tracking-[0.34em]">MONOLITH</div>
      </div>
      <div className="h-4 w-px bg-line-2" />
      <div className="text-[10px] uppercase tracking-[0.16em] text-txt-3">LOCAL VAULT · XCHACHA20</div>
      <div className="flex-1" />
      <button
        onClick={onToggleLock}
        className="flex cursor-pointer items-center gap-[7px] border-none bg-transparent px-1.5 text-[10px] uppercase tracking-[0.14em] text-txt-2"
      >
        <span
          className="size-1.5"
          style={{
            background: locked ? "var(--danger)" : "var(--ok)",
            boxShadow: `0 0 8px ${locked ? "var(--danger)" : "var(--ok)"}`,
          }}
        />
        {locked ? "LOCKED" : "UNLOCKED"}
      </button>
      <div className="pr-1 text-[10px] uppercase tracking-[0.16em] text-txt-3">SYNC&nbsp;OFF</div>
      <div className="flex h-full">
        <button
          title="Minimize"
          onClick={() => void winMinimize()}
          className="grid h-9 w-[46px] cursor-pointer place-items-center border-l border-line bg-transparent text-txt-2 transition-colors duration-100 hover:bg-bg-2 hover:text-txt"
        >
          <svg viewBox="0 0 11 11" className="size-[11px]">
            <path d="M1 6h9" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
        <button
          title="Maximize"
          onClick={() => void winToggleMaximize()}
          className="grid h-9 w-[46px] cursor-pointer place-items-center border-l border-line bg-transparent text-txt-2 transition-colors duration-100 hover:bg-bg-2 hover:text-txt"
        >
          <svg viewBox="0 0 11 11" className="size-[11px]">
            <rect x="1.5" y="1.5" width="8" height="8" fill="none" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
        <button
          title="Close"
          onClick={() => void winClose()}
          className="grid h-9 w-[46px] cursor-pointer place-items-center border-l border-line bg-transparent text-txt-2 transition-colors duration-100 hover:bg-danger hover:text-white"
        >
          <svg viewBox="0 0 11 11" className="size-[11px]">
            <path d="M1.5 1.5l8 8M9.5 1.5l-8 8" stroke="currentColor" strokeWidth="1.2" />
          </svg>
        </button>
      </div>
    </div>
  );
}

const NAV: { key: View; label: string; icon: string }[] = [
  { key: "home", label: "Projects", icon: "dash" },
  { key: "browse", label: "All Items", icon: "list" },
  { key: "settings", label: "Settings", icon: "gear" },
];

export function Sidebar({
  view,
  setView,
  projects,
  onOpenProject,
  activeProjectId,
  onNewProject,
  onEditProject,
  onDeleteProject,
  onSetIcon,
  storage,
}: {
  view: View;
  setView: (v: View) => void;
  projects: Project[];
  onOpenProject: (p: Project) => void;
  activeProjectId?: string;
  onNewProject: () => void;
  onEditProject: (p: Project) => void;
  onDeleteProject: (p: Project) => void | Promise<void>;
  onSetIcon: (id: string, icon: ProjectIconData | null) => void;
  storage: Storage;
}) {
  const personal = projects.find((p) => p.personal);
  const regularProjects = projects.filter((p) => !p.personal);
  return (
    <aside className="fixed inset-x-0 bottom-0 z-40 flex h-[calc(4rem+env(safe-area-inset-bottom))] border-t border-line bg-bg-1 pb-[env(safe-area-inset-bottom)] md:relative md:inset-auto md:z-auto md:h-auto md:min-h-0 md:flex-col md:overflow-hidden md:border-t-0 md:border-r md:pb-0">
      <nav className="grid w-full grid-cols-3 gap-px px-2 py-2 md:flex md:w-auto md:flex-col md:border-b md:border-line md:px-2.5 md:py-3">
        {NAV.map((n) => {
          const on = view === n.key || (n.key === "home" && view === "project");
          return (
            <button
              key={n.key}
              onClick={() => setView(n.key)}
              className={
                "flex cursor-pointer flex-col items-center justify-center gap-1 border px-2 py-2 text-[9px] uppercase tracking-[0.08em] transition-all duration-[120ms] md:flex-row md:justify-start md:gap-[11px] md:px-[11px] md:py-[9px] md:text-[11px] md:tracking-[0.14em] " +
                (on
                  ? "border-acc-line bg-acc-dim text-acc"
                  : "border-transparent bg-transparent text-txt-2 hover:bg-bg-2")
              }
            >
              <Icon name={n.icon} size={16} />
              <span>{n.label}</span>
              {on && <span className="hidden md:ml-auto md:block md:size-1 md:bg-acc" />}
            </button>
          );
        })}
      </nav>

      <div className="hidden flex-1 overflow-y-auto overflow-x-hidden px-2.5 pb-2.5 pt-3.5 md:block">
        {personal && (
          <div className="mb-3 border-b border-line pb-3">
            <Lbl className="px-1 pb-[9px]">Personal</Lbl>
            <SidebarProjectButton
              project={personal}
              active={activeProjectId === personal.id && view === "project"}
              onOpen={() => onOpenProject(personal)}
              onEditProject={onEditProject}
              onDeleteProject={onDeleteProject}
              onSetIcon={onSetIcon}
            />
          </div>
        )}
        <Lbl className="flex items-center justify-between px-1 pb-[9px]">
          <span>Projects</span>
          <button
            onClick={onNewProject}
            title="New project"
            className="flex cursor-pointer border-none bg-transparent p-0 text-txt-3 hover:text-acc"
          >
            <Icon name="plus" size={13} />
          </button>
        </Lbl>
        {regularProjects.map((p) => (
          <SidebarProjectButton
            key={p.id}
            project={p}
            active={activeProjectId === p.id && view === "project"}
            onOpen={() => onOpenProject(p)}
            onEditProject={onEditProject}
            onDeleteProject={onDeleteProject}
            onSetIcon={onSetIcon}
          />
        ))}

        <button
          type="button"
          onClick={onNewProject}
          className="mt-3.5 w-full cursor-pointer border border-dashed border-line-2 bg-transparent px-3 py-[18px] text-center text-txt-3 transition-all duration-[120ms] hover:border-acc-line hover:text-acc"
        >
          <div className="mb-2 flex justify-center">
            <Icon name="plus" size={18} />
          </div>
          <Lbl className="leading-[1.7] text-current">NEW PROJECT</Lbl>
        </button>
      </div>

      <div className="hidden border-t border-line px-3.5 py-3 md:block">
        <div className="mb-[7px] flex justify-between">
          <LblText>Local store</LblText>
          <span className="text-[10px] tabular-nums text-txt-2">
            {storage.used} / {storage.total}
          </span>
        </div>
        <div className="flex h-[5px] bg-line-2">
          <div
            className="bg-acc shadow-[0_0_10px_var(--accent-dim)]"
            style={{ width: `${storage.pct}%` }}
          />
        </div>
      </div>
    </aside>
  );
}

function SidebarProjectButton({
  project,
  active,
  onOpen,
  onEditProject,
  onDeleteProject,
  onSetIcon,
}: {
  project: Project;
  active: boolean;
  onOpen: () => void;
  onEditProject: (p: Project) => void;
  onDeleteProject: (p: Project) => void | Promise<void>;
  onSetIcon: (id: string, icon: ProjectIconData | null) => void;
}) {
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onOpen();
    }
  };

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={onKeyDown}
      className={
        "group/sidebar-row mb-px flex w-full cursor-pointer items-center gap-2.5 border px-2 py-[7px] text-left outline-none transition-all duration-[120ms] " +
        (active ? "border-line-2 bg-bg-3" : "border-transparent bg-transparent hover:bg-bg-2")
      }
    >
      <ProjectIcon project={project} size={22} radius={0} onSetIcon={onSetIcon} />
      <span
        className={
          "min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[11.5px] " +
          (active ? "text-txt" : "text-txt-2")
        }
      >
        {project.name}
      </span>
      <span className="text-[10px] tabular-nums text-txt-3">{project.count}</span>
      <ProjectActionsMenu
        project={project}
        onOpen={() => onOpen()}
        onEditProject={onEditProject}
        onDeleteProject={onDeleteProject}
        onSetIcon={onSetIcon}
        className="opacity-100 md:opacity-0 md:group-hover/sidebar-row:opacity-100 md:group-focus-within/sidebar-row:opacity-100"
      />
    </div>
  );
}
