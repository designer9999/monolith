/**
 * Project icon: monogram / glyph / uploaded image, with a right-click context
 * menu to change it. Icon changes persist to the Rust core via `setProjectIcon`.
 * Ported 1:1 from the design's project-icon.jsx.
 */

import { useRef, useState } from "react";

import type { Project, ProjectIcon as ProjectIconData } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { Btn } from "@/components/ui/btn";
import { Lbl } from "@/components/ui/primitives";

const GLYPHS = ["folder", "vault", "layers", "shield", "globe", "terminal", "star", "key", "card", "qr"];
const ICON_COLORS = [
  "#5b9dff", "#c8ff2e", "#ff8a3d", "#b98cff", "#34e29a",
  "#ff5a52", "#60a5fa", "#f6821f", "#e8edf2", "#6b7280",
];

interface Resolved {
  kind: string;
  name: string;
  src: string | null;
  color: string;
}

function resolveIcon(p: Project): Resolved {
  const ic = p.icon ?? ({} as ProjectIconData);
  return {
    kind: ic.kind || "mono",
    name: ic.name || "folder",
    src: ic.src || null,
    color: ic.color || p.color,
  };
}

export function ProjectIcon({
  project,
  size = 42,
  radius,
  onSetIcon,
  menu = true,
}: {
  project: Project;
  size?: number;
  radius?: number;
  onSetIcon?: (id: string, icon: ProjectIconData | null) => void;
  menu?: boolean;
}) {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const ic = resolveIcon(project);
  const fs = Math.round(size * 0.38);

  const openMenu = (e: React.MouseEvent) => {
    if (!menu || !onSetIcon) return;
    e.preventDefault();
    e.stopPropagation();
    const mw = 248;
    const mh = 300;
    setPos({
      x: Math.min(e.clientX, window.innerWidth - mw - 8),
      y: Math.min(e.clientY, window.innerHeight - mh - 8),
    });
  };

  const editable = menu && !!onSetIcon;

  return (
    <>
      <div
        onContextMenu={openMenu}
        title={editable ? "Click the pencil (or right-click) to change icon" : undefined}
        className={`group/icon relative grid flex-none place-items-center overflow-hidden text-acc-ink ${editable ? "cursor-context-menu" : "cursor-default"}`}
        style={{
          width: size,
          height: size,
          borderRadius: radius != null ? radius : "var(--radius)",
          background: ic.kind === "img" ? "var(--bg-2)" : ic.color,
          boxShadow: `0 0 18px ${ic.color}33`,
        }}
      >
        {ic.kind === "img" && ic.src ? (
          <img src={ic.src} alt="" draggable={false} className="block h-full w-full object-cover" />
        ) : ic.kind === "glyph" ? (
          <span className="flex">
            <Icon name={ic.name} size={Math.round(size * 0.5)} stroke={1.8} />
          </span>
        ) : (
          <span className="font-display font-bold" style={{ fontSize: fs }}>
            {project.mono}
          </span>
        )}
        {/* Visible edit affordance (right-click still works too). Only on larger icons. */}
        {editable && size >= 40 && (
          <button
            type="button"
            aria-label="Change project icon"
            title="Change icon"
            onClick={openMenu}
            className="absolute bottom-0 right-0 grid size-4 place-items-center bg-bg-1/90 text-txt opacity-0 transition-opacity group-hover/icon:opacity-100"
          >
            <Icon name="pencil" size={10} />
          </button>
        )}
      </div>

      {onSetIcon && (
        <ProjectIconPicker
          project={project}
          pos={pos}
          onClose={() => setPos(null)}
          onSetIcon={onSetIcon}
        />
      )}
    </>
  );
}

export function ProjectIconPicker({
  project,
  pos,
  onClose,
  onSetIcon,
}: {
  project: Project;
  pos: { x: number; y: number } | null;
  onClose: () => void;
  onSetIcon: (id: string, icon: ProjectIconData | null) => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const ic = resolveIcon(project);

  if (!pos) return null;

  const set = (patch: Partial<ProjectIconData>) => {
    onSetIcon(project.id, { kind: ic.kind, name: ic.name, src: ic.src ?? undefined, color: ic.color, ...patch });
  };

  // Icons are stored as data URLs in (unencrypted) project metadata, so keep them
  // small: cap the file size and only accept images.
  const MAX_ICON_BYTES = 256 * 1024;
  const onFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    e.target.value = "";
    if (!f) return;
    if (!f.type.startsWith("image/")) {
      setError("Choose an image file.");
      return;
    }
    if (f.size > MAX_ICON_BYTES) {
      setError("Image is too large. Pick one under 256 KB.");
      return;
    }
    setError(null);
    const r = new FileReader();
    r.onload = () => set({ kind: "img", src: r.result as string });
    r.readAsDataURL(f);
    onClose();
  };

  return (
    <div
      onMouseDown={onClose}
      onContextMenu={(e) => {
        e.preventDefault();
        onClose();
      }}
      className="fixed inset-0 z-[48]"
    >
      <div
        onMouseDown={(e) => e.stopPropagation()}
        className="animate-in fade-in fixed w-[248px] border border-line-2 bg-bg-1 p-3 shadow-[0_24px_60px_rgba(0,0,0,0.6)]"
        style={{ left: pos.x, top: pos.y }}
      >
        <Lbl className="mb-[9px]">Project icon</Lbl>
        {error && (
          <div className="mb-2 border border-danger bg-bg px-2 py-1.5 text-[10px] text-danger" role="alert">
            {error}
          </div>
        )}
        <Btn
          variant="ghost"
          onClick={() => fileRef.current?.click()}
          className="mb-1.5 w-full justify-center text-[10px]"
        >
          <Icon name="img" size={13} /> Upload image
        </Btn>
        <Lbl className="mb-2.5 text-[8px] leading-[1.4] text-txt-4">
          Max 256 KB · stored unencrypted as a label
        </Lbl>
        <input ref={fileRef} type="file" accept="image/*" onChange={onFile} className="hidden" />

        <Lbl className="mx-0 mt-1 mb-[7px] text-txt-4">Glyph</Lbl>
        <div className="mb-[11px] grid grid-cols-5 gap-px bg-line">
          {GLYPHS.map((g) => {
            const on = ic.kind === "glyph" && ic.name === g;
            return (
              <button
                key={g}
                onClick={() => set({ kind: "glyph", name: g })}
                className={`grid aspect-square cursor-pointer place-items-center border-none ${on ? "text-acc-ink" : "bg-bg-2 text-txt-2"}`}
                style={on ? { background: ic.color } : undefined}
              >
                <Icon name={g} size={15} />
              </button>
            );
          })}
          <button
            onClick={() => set({ kind: "mono" })}
            className={`grid aspect-square cursor-pointer place-items-center border-none font-display text-[11px] font-bold ${ic.kind === "mono" ? "text-acc-ink" : "bg-bg-2 text-txt-2"}`}
            style={ic.kind === "mono" ? { background: ic.color } : undefined}
          >
            {project.mono}
          </button>
        </div>

        <Lbl className="mx-0 mt-1 mb-[7px] text-txt-4">Color</Lbl>
        <div className="flex flex-wrap gap-1.5">
          {ICON_COLORS.map((c) => (
            <button
              key={c}
              onClick={() => set({ color: c })}
              className={`size-[22px] cursor-pointer border ${ic.color === c ? "border-2 border-txt" : "border-line-2"}`}
              style={{ background: c }}
            />
          ))}
        </div>

        <Btn
          variant="ghost"
          onClick={() => {
            onSetIcon(project.id, null);
            onClose();
          }}
          className="mt-3 w-full justify-center text-[10px] text-txt-3"
        >
          <Icon name="refresh" size={12} /> Reset to default
        </Btn>
      </div>
    </div>
  );
}
