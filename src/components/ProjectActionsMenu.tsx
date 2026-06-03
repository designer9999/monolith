import { useEffect, useState, type MouseEvent } from "react";

import type { AppError, Project } from "@/lib/types";
import type { ProjectIcon as ProjectIconData } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { cn } from "@/lib/utils";
import { Lbl } from "@/components/ui/primitives";
import { ProjectIconPicker } from "./ProjectIcon";

type ActionState = "open" | "edit" | "delete";

export function ProjectActionsMenu({
  project,
  onOpen,
  onEditProject,
  onDeleteProject,
  onSetIcon,
  className,
}: {
  project: Project;
  onOpen?: (project: Project) => void;
  onEditProject: (project: Project) => void;
  onDeleteProject: (project: Project) => void | Promise<void>;
  onSetIcon?: (id: string, icon: ProjectIconData | null) => void;
  className?: string;
}) {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const [iconPos, setIconPos] = useState<{ x: number; y: number } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [working, setWorking] = useState<ActionState | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!pos) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setPos(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pos]);

  const openMenu = (e: MouseEvent<HTMLButtonElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const rect = e.currentTarget.getBoundingClientRect();
    const width = 226;
    const height = project.personal ? 156 : 226;
    setPos({
      x: Math.min(Math.max(8, rect.right - width), window.innerWidth - width - 8),
      y: Math.min(rect.bottom + 6, window.innerHeight - height - 8),
    });
    setConfirmDelete(false);
    setError(null);
  };

  const run = (state: ActionState, action: () => void | Promise<void>) => {
    void (async () => {
      try {
        setWorking(state);
        setError(null);
        await action();
        setPos(null);
      } catch (err) {
        setError((err as AppError)?.message ?? "Action failed.");
      } finally {
        setWorking(null);
      }
    })();
  };

  const openIconPicker = () => {
    if (!pos) return;
    const width = 248;
    const height = 300;
    setIconPos({
      x: Math.min(pos.x, window.innerWidth - width - 8),
      y: Math.min(pos.y, window.innerHeight - height - 8),
    });
    setPos(null);
  };

  return (
    <>
      <button
        type="button"
        draggable={false}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={openMenu}
        aria-label={`Project actions for ${project.name}`}
        title="Project actions"
        className={cn(
          "grid size-7 flex-none place-items-center border border-transparent text-txt-3 transition-colors hover:border-line-2 hover:text-acc",
          pos && "border-line-2 text-acc",
          className,
        )}
      >
        <Icon name="more" size={14} />
      </button>

      {pos && (
        <div
          onMouseDown={() => setPos(null)}
          className="fixed inset-0 z-[49]"
        >
          <div
            onMouseDown={(e) => e.stopPropagation()}
            className="animate-in fade-in fixed w-[226px] border border-line-2 bg-bg-1 p-2 shadow-[0_24px_60px_rgba(0,0,0,0.6)]"
            style={{ left: pos.x, top: pos.y }}
          >
            <Lbl className="px-2 py-1.5">{project.name}</Lbl>
            {error && (
              <div className="mx-2 mb-1.5 border border-danger bg-bg px-2 py-1.5 text-[10px] text-danger" role="alert">
                {error}
              </div>
            )}
            {onOpen && (
              <MenuItem
                icon="arrow"
                label={working === "open" ? "Opening..." : "Open"}
                disabled={!!working}
                onClick={() => run("open", () => onOpen(project))}
              />
            )}
            <MenuItem
              icon="pencil"
              label="Edit details"
              disabled={!!working}
              onClick={() => run("edit", () => onEditProject(project))}
            />
            {onSetIcon && (
              <MenuItem
                icon="img"
                label="Change icon"
                disabled={!!working}
                onClick={openIconPicker}
              />
            )}
            {project.personal ? (
              <div className="mt-1 border-t border-line px-2 py-2 font-mono text-[10px] uppercase tracking-[0.12em] text-txt-4">
                Protected vault
              </div>
            ) : confirmDelete ? (
              <div className="mt-1 border-t border-line pt-1">
                <MenuItem
                  icon="trash"
                  label={working === "delete" ? "Deleting..." : "Confirm delete"}
                  danger
                  disabled={!!working}
                  onClick={() => run("delete", () => onDeleteProject(project))}
                />
                <MenuItem
                  icon="x"
                  label="Cancel"
                  disabled={!!working}
                  onClick={() => setConfirmDelete(false)}
                />
              </div>
            ) : (
              <div className="mt-1 border-t border-line pt-1">
                <MenuItem
                  icon="trash"
                  label="Delete project"
                  danger
                  disabled={!!working}
                  onClick={() => setConfirmDelete(true)}
                />
              </div>
            )}
          </div>
        </div>
      )}
      {onSetIcon && (
        <ProjectIconPicker
          project={project}
          pos={iconPos}
          onClose={() => setIconPos(null)}
          onSetIcon={onSetIcon}
        />
      )}
    </>
  );
}

function MenuItem({
  icon,
  label,
  danger,
  disabled,
  onClick,
}: {
  icon: string;
  label: string;
  danger?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex w-full items-center gap-2 border border-transparent px-2 py-2 text-left font-mono text-[10px] uppercase tracking-[0.12em] transition-colors disabled:pointer-events-none disabled:opacity-50",
        danger
          ? "text-danger hover:border-danger hover:bg-danger/10"
          : "text-txt-2 hover:border-line-2 hover:bg-bg-2 hover:text-txt",
      )}
    >
      <Icon name={icon} size={12} />
      <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{label}</span>
    </button>
  );
}
