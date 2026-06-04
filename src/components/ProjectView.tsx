/**
 * Project detail: collapsible service panels (env chips, expiration/2FA badges,
 * inline TOTP, reveal/copy fields), plus attachment metadata.
 * Ported 1:1 from the design's project.jsx; services + fields come from the vault.
 */

import { useEffect, useMemo, useState, type MouseEvent } from "react";

import {
  addAttachment,
  deleteService,
  listPasswordHistory,
  listServices,
  revealHistory,
  updateService,
} from "@/lib/tauri";
import type { AppError, Attachment, Environment, FieldView, PasswordHistoryEntry, Project, Service } from "@/lib/types";
import type { ProjectIcon as ProjectIconData } from "@/lib/types";
import { Icon } from "@/lib/icons";
import { fmtDate } from "@/lib/format";
import { expirationInfo, isExpirationAttention } from "@/lib/expiration";
import { ServiceMark, useCopy } from "@/lib/ui";
import { Btn } from "@/components/ui/btn";
import { Chip, Lbl, LblText, MiniStat } from "@/components/ui/primitives";
import { ProjectIcon } from "./ProjectIcon";
import { ProjectActionsMenu } from "./ProjectActionsMenu";
import { FieldRow } from "./FieldRow";
import { Totp } from "./Totp";
import { EditService, type ServiceDraft } from "./modals";

const ENV_META: Record<Environment, { c: string; l: string }> = {
  production: { c: "var(--ok)", l: "PROD" },
  staging: { c: "var(--warn)", l: "STAGING" },
  dev: { c: "var(--info)", l: "DEV" },
  all: { c: "var(--txt-3)", l: "ALL ENV" },
};

function EnvChip({ env }: { env: Environment }) {
  const m = ENV_META[env] || ENV_META.all;
  // Per-instance env color from data → inline color drives the current-tone border.
  return (
    <Chip tone="current" style={{ color: m.c }}>
      <span className="size-1.5" style={{ background: m.c }} />
      {m.l}
    </Chip>
  );
}

function serviceSearchText(service: Service): string {
  return [
    service.title,
    service.templateName,
    service.templateId,
    service.label,
    service.env,
    service.group,
    service.expiresAt ?? "",
    ...service.fields
      .filter((field) => field.hasValue)
      .flatMap((field) => [
        field.label,
        field.fieldType,
        field.secret ? "" : field.value ?? "",
      ]),
  ]
    .join(" ")
    .toLowerCase();
}

export function ProjectView({
  project,
  focusId,
  onBack,
  onAddService,
  onEditProject,
  onDeleteProject,
  onSetIcon,
  reloadKey,
  revealSecretsByDefault,
}: {
  project: Project;
  focusId?: string | null;
  onBack: () => void;
  onAddService: (p: Project) => void;
  onEditProject: (p: Project) => void;
  onDeleteProject: (p: Project) => void | Promise<void>;
  onSetIcon: (id: string, icon: ProjectIconData | null) => void;
  reloadKey: number;
  revealSecretsByDefault: boolean;
}) {
  const [services, setServices] = useState<Service[]>([]);
  const [files, setFiles] = useState<Attachment[]>(project.files || []);
  const [dragF, setDragF] = useState(false);
  const [fileError, setFileError] = useState<string | null>(null);
  const [serviceError, setServiceError] = useState<string | null>(null);
  const [loadingServices, setLoadingServices] = useState(false);
  const [editingService, setEditingService] = useState<Service | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (!editingService) return;
    const current = window.history.state as Record<string, unknown> | null;
    if (current?.monolith && current.editServiceId !== editingService.id) {
      window.history.pushState({ ...current, editServiceId: editingService.id }, "");
    }
    const closeOnBack = (event: PopStateEvent) => {
      const state = event.state as Record<string, unknown> | null;
      if (state?.editServiceId !== editingService.id) {
        setEditingService(null);
      }
    };
    window.addEventListener("popstate", closeOnBack);
    return () => window.removeEventListener("popstate", closeOnBack);
  }, [editingService?.id]);

  useEffect(() => {
    let alive = true;
    setLoadingServices(true);
    setServiceError(null);
    void listServices(project.id)
      .then((s) => {
        if (alive) setServices(s);
      })
      .catch((err) => {
        if (alive) setServiceError((err as AppError)?.message ?? "Couldn't load services.");
      })
      .finally(() => {
        if (alive) setLoadingServices(false);
      });
    return () => {
      alive = false;
    };
  }, [project.id, reloadKey]);

  const onRemoveService = async (serviceId: string) => {
    try {
      setServiceError(null);
      await deleteService(serviceId);
      setServices(await listServices(project.id));
    } catch (err) {
      setServiceError((err as AppError)?.message ?? "Couldn't remove the service.");
      throw err;
    }
  };

  useEffect(() => {
    setFiles(project.files || []);
  }, [project.id, project.files]);

  const filteredServices = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return services;
    return services.filter((service) => serviceSearchText(service).includes(q));
  }, [services, query]);
  const totp = services.filter((s) => s.totp).length;
  const risk = services.filter((s) => isExpirationAttention(s.expiresAt) || s.exposed || s.reused).length;

  const onEditService = async (service: Service, draft: ServiceDraft) => {
    try {
      setServiceError(null);
      const updated = await updateService({
        serviceId: service.id,
        label: draft.label ?? "",
        env: draft.env ?? service.env,
        expiresAt: draft.expiresAt ?? "",
        fields: draft.fields,
        totpSecret: draft.totpSecret,
      });
      setServices((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
      setEditingService(null);
    } catch (err) {
      setServiceError((err as AppError)?.message ?? "Couldn't edit the service.");
      throw err;
    }
  };

  const onEditField = async (service: Service, field: FieldView, value: string) => {
    try {
      setServiceError(null);
      const updated = await updateService({
        serviceId: service.id,
        label: service.label,
        env: service.env,
        expiresAt: service.expiresAt ?? "",
        fields: [{ label: field.label, value }],
      });
      setServices((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
    } catch (err) {
      setServiceError((err as AppError)?.message ?? "Couldn't edit the field.");
      throw err;
    }
  };

  const onDropFiles = async (e: React.DragEvent) => {
    e.preventDefault();
    setDragF(false);
    setFileError(null);
    const dropped = [...(e.dataTransfer.files || [])];
    for (const f of dropped) {
      const size = f.size / 1024 < 1024 ? `${(f.size / 1024).toFixed(1)} KB` : `${(f.size / 1048576).toFixed(1)} MB`;
      try {
        const att = await addAttachment(project.id, f.name, size);
        setFiles((prev) => [att, ...prev]);
      } catch (err) {
        setFileError((err as AppError)?.message ?? `Couldn't add ${f.name}.`);
      }
    }
  };

  return (
    <>
    <div className="h-full overflow-y-auto animate-in fade-in">
      <div className="border-b border-line px-4 pt-5 pb-4 sm:px-[30px] sm:pb-[22px]">
        <button
          onClick={onBack}
          className="flex items-center gap-[7px] bg-none border-none text-txt-3 cursor-pointer font-mono text-[10px] tracking-[0.16em] uppercase mb-[18px] p-0 transition-colors hover:text-acc"
        >
          <Icon name="back" size={13} /> All projects
        </button>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:gap-[18px]">
          <ProjectIcon project={project} size={54} onSetIcon={onSetIcon} />
          <div className="flex-1">
            <h1 className="font-display text-[24px] font-bold tracking-normal sm:text-[28px]">{project.name}</h1>
            <Lbl className="mt-[5px]">
              {project.sub} · CREATED {fmtDate(project.created)}
            </Lbl>
          </div>
          <div className="flex w-full gap-2 sm:w-auto">
            <Btn variant="ghost" onClick={() => onEditProject(project)}>
              <Icon name="pencil" size={13} /> Edit
            </Btn>
            <ProjectActionsMenu
              project={project}
              onEditProject={onEditProject}
              onDeleteProject={onDeleteProject}
              onSetIcon={onSetIcon}
              className="h-auto min-h-[34px] w-[38px] border-line text-txt-2 hover:bg-bg-2 hover:text-txt"
            />
            <Btn variant="primary" className="flex-1 justify-center sm:flex-none" onClick={() => onAddService(project)}>
              <Icon name="plus" size={14} /> Add Service
            </Btn>
          </div>
        </div>
        <div className="mt-5 flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
          <div className="grid grid-cols-3 gap-4 sm:flex sm:gap-7">
            <MiniStat n={services.length} label="Services" />
            <MiniStat n={totp} label="2FA enabled" tone={totp ? "accent" : null} />
            <MiniStat n={risk} label="Attention" tone={risk ? "danger" : "ok"} />
          </div>
          <div className="flex w-full items-center gap-[9px] border border-line-2 bg-bg px-3 py-[10px] lg:max-w-[540px]">
            <Icon name="search" size={14} className="text-txt-3" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="SEARCH THIS PROJECT..."
              className="min-w-0 flex-1 border-none bg-transparent font-mono text-[11px] uppercase tracking-[0.08em] text-txt outline-none placeholder:text-txt-4"
            />
            {query && (
              <button type="button" onClick={() => setQuery("")} className="flex text-txt-3 hover:text-txt">
                <Icon name="x" size={12} />
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="flex flex-col gap-px bg-line p-3 sm:p-6">
        {serviceError && (
          <div className="flex items-center gap-2 border border-danger bg-bg-1 px-4 py-3 text-[12px] text-danger" role="alert">
            <Icon name="warn" size={13} /> {serviceError}
          </div>
        )}
        {loadingServices && (
          <div className="bg-bg-1 px-4 py-3 font-mono text-[11px] uppercase tracking-[0.12em] text-txt-3">
            Loading services…
          </div>
        )}
        {filteredServices.length === 0 && !loadingServices && (
          <div className="bg-bg-1 px-4 py-10 text-center">
            <div className="mx-auto mb-4 grid size-14 place-items-center border border-dashed border-line-2 text-txt-3">
              <Icon name="search" size={22} />
            </div>
            <LblText className="text-txt-3">
              {query.trim() ? `NO MATCH FOR "${query.trim().toUpperCase()}"` : "NO SERVICES YET"}
            </LblText>
          </div>
        )}
        {filteredServices.map((s) => (
          <ServicePanel
            key={s.id}
            service={s}
            startOpen={filteredServices.length <= 4 || s.id === focusId}
            highlight={s.id === focusId}
            onRemove={onRemoveService}
            onEdit={() => setEditingService(s)}
            onFieldSave={(field, value) => onEditField(s, field, value)}
            revealSecretsByDefault={revealSecretsByDefault}
          />
        ))}
        <button
          onClick={() => onAddService(project)}
          className="flex cursor-pointer items-center justify-center gap-[11px] border border-dashed border-line-2 bg-bg-1 p-[18px] text-txt-3 transition-all hover:border-acc-line hover:text-acc"
        >
          <Icon name="plus" size={16} />
          <LblText>ADD SERVICE FROM TEMPLATE</LblText>
        </button>

        <div className="bg-bg-1 px-5 py-[18px] mt-px">
          <div className="flex items-center gap-[9px] mb-3.5">
            <span className="text-txt-3">
              <Icon name="layers" size={14} />
            </span>
            <LblText className="flex-1 text-txt-2">Files &amp; attachments</LblText>
            <LblText className="hidden text-txt-4 sm:inline">{files.length} · METADATA ONLY · ENCRYPTION PLANNED</LblText>
          </div>
          {fileError && (
            <div className="mb-3 flex items-center gap-2 text-[11px] text-danger" role="alert">
              <Icon name="warn" size={12} /> {fileError}
            </div>
          )}
          <div className="grid grid-cols-1 gap-px bg-line sm:[grid-template-columns:repeat(auto-fill,minmax(220px,1fr))]">
            {files.map((f) => (
              <div key={f.id} className="bg-bg-2 px-3.5 py-[13px] flex items-center gap-[11px]">
                <span className="size-[30px] flex-none grid place-items-center border border-line-2 text-txt-3">
                  <Icon name="note" size={14} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="font-mono text-[12px] text-txt overflow-hidden text-ellipsis whitespace-nowrap">
                    {f.name}
                  </div>
                  <Lbl className="text-txt-4 mt-[3px]">{f.size} · {fmtDate(f.date)}</Lbl>
                </div>
                <span className="text-txt-4" title="Metadata only — file encryption not implemented yet">
                  <Icon name="note" size={13} />
                </span>
              </div>
            ))}
            <div
              onDragOver={(e) => {
                e.preventDefault();
                setDragF(true);
              }}
              onDragLeave={() => setDragF(false)}
              onDrop={onDropFiles}
              className={`bg-bg-1 border border-dashed px-3.5 py-[13px] min-h-[58px] flex items-center justify-center gap-2.5 transition-all ${
                dragF ? "border-acc text-acc" : "border-line-2 text-txt-3"
              }`}
            >
              <Icon name="upload" size={15} />
              <LblText className={dragF ? "text-acc" : undefined}>{dragF ? "RELEASE TO ADD" : "DROP FILES"}</LblText>
            </div>
          </div>
        </div>
      </div>
    </div>
    {editingService && (
      <EditService
        service={editingService}
        onClose={() => setEditingService(null)}
        onSave={(draft) => onEditService(editingService, draft)}
        revealSecretsByDefault={revealSecretsByDefault}
      />
    )}
    </>
  );
}

function ExpirationChip({ expiresAt }: { expiresAt?: string | null }) {
  const exp = expirationInfo(expiresAt);
  if (exp.tone === "none") return null;
  if (exp.tone === "expired") {
    return (
      <Chip tone="danger">
        <Icon name="warn" size={10} /> EXPIRED
      </Chip>
    );
  }
  if (exp.tone === "soon") {
    return (
      <Chip tone="warn">
        <Icon name="clock" size={10} /> {exp.label.toUpperCase()}
      </Chip>
    );
  }
  return (
    <Chip tone="default">
      <Icon name="clock" size={10} /> {exp.label.toUpperCase()}
    </Chip>
  );
}

function ServicePanel({
  service,
  startOpen,
  highlight,
  onRemove,
  onEdit,
  onFieldSave,
  revealSecretsByDefault,
}: {
  service: Service;
  startOpen: boolean;
  highlight: boolean;
  onRemove: (serviceId: string) => void | Promise<void>;
  onEdit: () => void;
  onFieldSave: (field: FieldView, value: string) => void | Promise<void>;
  revealSecretsByDefault: boolean;
}) {
  const [open, setOpen] = useState(startOpen);
  const [removing, setRemoving] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [menuPos, setMenuPos] = useState<{ x: number; y: number } | null>(null);
  const [menuConfirmRemove, setMenuConfirmRemove] = useState(false);
  const [menuError, setMenuError] = useState<string | null>(null);
  useEffect(() => {
    if (highlight) setOpen(true);
  }, [highlight]);
  useEffect(() => {
    if (!menuPos) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenuPos(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [menuPos]);
  const [copied, copy] = useCopy();
  const exp = expirationInfo(service.expiresAt);
  const risky = exp.tone === "expired" || exp.tone === "soon" || service.exposed || service.reused;
  const urgent = service.exposed || exp.tone === "expired";
  const visibleFields = service.fields.filter((field) => field.hasValue);
  const attentionText = service.exposed
    ? "Credential found in a known breach — rotate now."
    : service.reused
      ? "This password is reused in another service."
      : exp.tone === "expired"
        ? "This credential is past its expiration date."
        : exp.tone === "soon"
          ? "This credential is approaching its expiration date."
          : "";

  const openContextMenu = (event: MouseEvent<HTMLElement>) => {
    event.preventDefault();
    event.stopPropagation();
    const width = 226;
    const height = 180;
    setMenuPos({
      x: Math.min(Math.max(8, event.clientX), window.innerWidth - width - 8),
      y: Math.min(Math.max(8, event.clientY), window.innerHeight - height - 8),
    });
    setMenuConfirmRemove(false);
    setMenuError(null);
  };

  const removeService = async () => {
    try {
      setRemoving(true);
      setMenuError(null);
      await onRemove(service.id);
      setMenuPos(null);
      setConfirmRemove(false);
      setMenuConfirmRemove(false);
    } catch (err) {
      setMenuError((err as AppError)?.message ?? "Couldn't remove the service.");
    } finally {
      setRemoving(false);
    }
  };

  return (
    <div className={`bg-bg-1 ${highlight ? "shadow-[inset_0_0_0_1px_var(--accent-line)]" : ""}`}>
      <button
        onClick={() => setOpen((o) => !o)}
        onContextMenu={openContextMenu}
        className="flex w-full cursor-pointer items-center gap-3 border-none bg-transparent px-4 py-[14px] text-left sm:gap-3.5 sm:px-[18px] sm:py-[15px]"
      >
        <ServiceMarkFromService service={service} />
        <div className="flex-1 min-w-0">
          <div className="flex flex-wrap items-center gap-2 sm:gap-[9px]">
            <span className="font-display text-[14px] font-semibold sm:text-[15px]">{service.templateName}</span>
            {service.label && <Chip>{service.label}</Chip>}
            <EnvChip env={service.env} />
            <ExpirationChip expiresAt={service.expiresAt} />
            {service.totp && (
              <Chip tone="accent">
                <Icon name="refresh" size={10} /> 2FA
              </Chip>
            )}
            {service.danger && (
              <Chip tone="default">
                <Icon name="key" size={10} /> SENSITIVE
              </Chip>
            )}
            {risky && (
              <span className={service.exposed || exp.tone === "expired" ? "text-danger" : "text-warn"}>
                <Icon name="warn" size={13} />
              </span>
            )}
          </div>
          <Lbl className="mt-[5px] text-txt-4">
            {visibleFields.length} FIELDS · UPDATED {fmtDate(service.updated)}
          </Lbl>
        </div>
        <span className={`text-txt-3 transition-transform duration-150 ${open ? "rotate-180" : ""}`}>
          <Icon name="chevd" size={16} />
        </span>
      </button>

      {open && (
        <div className="animate-in fade-in border-t border-line px-4 pb-[18px] sm:px-[18px]">
          {risky && (
            // Danger vs warn box border/bg/text is data-driven → keep those colors inline.
            <div
              className="mt-4 mb-1 flex items-start gap-2.5 border px-3 py-[11px] sm:items-center"
              style={{
                borderColor: urgent ? "var(--danger)" : "var(--warn)",
                background: urgent ? "rgba(255,90,82,0.08)" : "rgba(255,176,46,0.08)",
                color: urgent ? "var(--danger)" : "var(--warn)",
              }}
            >
              <Icon name="warn" size={15} />
              <span className="text-[11px] tracking-[0.03em]">
                {attentionText}
              </span>
            </div>
          )}
          {service.totp && (
            <div className="mt-4">
              <Totp serviceId={service.id} copy={copy} copied={copied} />
            </div>
          )}
          <Lbl className="pt-[18px] pb-0.5">Credentials</Lbl>
          {visibleFields.length ? (
            visibleFields.map((f, i) => (
              <FieldRow
                key={f.id}
                field={f}
                idx={i}
                copy={copy}
                copied={copied}
                revealByDefault={revealSecretsByDefault}
                onSave={onFieldSave}
              />
            ))
          ) : (
            <div className="border-b border-line py-4 font-mono text-[10px] uppercase tracking-[0.12em] text-txt-4">
              No populated fields yet.
            </div>
          )}
          <PasswordArchive serviceId={service.id} copy={copy} copied={copied} />
          <div className="mt-4 flex items-center gap-2">
            <Btn variant="ghost" className="text-[10px]" onClick={onEdit}>
              <Icon name="pencil" size={12} /> Edit
            </Btn>
            <div className="flex-1" />
            {confirmRemove ? (
              <>
                <Btn
                  variant="ghost"
                  className="text-[10px]"
                  onClick={() => setConfirmRemove(false)}
                  disabled={removing}
                >
                  Cancel
                </Btn>
                <Btn
                  variant="danger"
                  className="text-[10px]"
                  onClick={() => void removeService()}
                  disabled={removing}
                >
                  <Icon name="x" size={12} /> {removing ? "Removing…" : "Confirm"}
                </Btn>
              </>
            ) : (
              <Btn
                variant="ghost"
                className="text-[10px] text-danger border-line"
                onClick={() => setConfirmRemove(true)}
              >
                <Icon name="x" size={12} /> Remove
              </Btn>
            )}
          </div>
        </div>
      )}
      {menuPos && (
        <div
          className="fixed inset-0 z-[49]"
          onMouseDown={() => setMenuPos(null)}
          onContextMenu={(event) => {
            event.preventDefault();
            setMenuPos(null);
          }}
        >
          <div
            className="animate-in fade-in fixed w-[226px] border border-line-2 bg-bg-1 p-2 shadow-[0_24px_60px_rgba(0,0,0,0.6)]"
            style={{ left: menuPos.x, top: menuPos.y }}
            onMouseDown={(event) => event.stopPropagation()}
            onContextMenu={(event) => event.preventDefault()}
          >
            <Lbl className="px-2 py-1.5">{service.title}</Lbl>
            {menuError && (
              <div className="mx-2 mb-1.5 border border-danger bg-bg px-2 py-1.5 text-[10px] text-danger" role="alert">
                {menuError}
              </div>
            )}
            <ServiceMenuItem
              icon="pencil"
              label="Edit service"
              disabled={removing}
              onClick={() => {
                setMenuPos(null);
                onEdit();
              }}
            />
            <ServiceMenuItem
              icon={open ? "chevd" : "chev"}
              label={open ? "Collapse" : "Expand"}
              disabled={removing}
              onClick={() => {
                setOpen((current) => !current);
                setMenuPos(null);
              }}
            />
            {menuConfirmRemove ? (
              <div className="mt-1 border-t border-line pt-1">
                <ServiceMenuItem
                  icon="trash"
                  label={removing ? "Removing..." : "Confirm remove"}
                  danger
                  disabled={removing}
                  onClick={() => void removeService()}
                />
                <ServiceMenuItem
                  icon="x"
                  label="Cancel"
                  disabled={removing}
                  onClick={() => setMenuConfirmRemove(false)}
                />
              </div>
            ) : (
              <div className="mt-1 border-t border-line pt-1">
                <ServiceMenuItem
                  icon="trash"
                  label="Remove service"
                  danger
                  disabled={removing}
                  onClick={() => setMenuConfirmRemove(true)}
                />
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ServiceMenuItem({
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
      className={`flex w-full items-center gap-2 border border-transparent px-2 py-2 text-left font-mono text-[10px] uppercase tracking-[0.12em] transition-colors disabled:pointer-events-none disabled:opacity-50 ${
        danger
          ? "text-danger hover:border-danger hover:bg-danger/10"
          : "text-txt-2 hover:border-line-2 hover:bg-bg-2 hover:text-txt"
      }`}
    >
      <Icon name={icon} size={12} />
      <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{label}</span>
    </button>
  );
}

function PasswordArchive({
  serviceId,
  copy,
  copied,
}: {
  serviceId: string;
  copy: (text: string, key?: string) => void;
  copied: string | true | null;
}) {
  const [entries, setEntries] = useState<PasswordHistoryEntry[]>([]);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void listPasswordHistory(serviceId)
      .then((history) => {
        if (alive) setEntries(history);
      })
      .catch((err) => {
        if (alive) setError((err as AppError)?.message ?? "Couldn't load password archive.");
      });
    return () => {
      alive = false;
    };
  }, [serviceId]);

  const reveal = async (entry: PasswordHistoryEntry) => {
    setError(null);
    const value = (await revealHistory(entry.id)).value;
    setRevealed((current) => ({ ...current, [entry.id]: value }));
    return value;
  };

  if (entries.length === 0 && !error) return null;

  return (
    <div className="mt-4 border-t border-line pt-4">
      <Lbl className="pb-1">Password archive</Lbl>
      {error && <div className="py-2 text-[10px] text-danger">{error}</div>}
      <div className="flex flex-col gap-px bg-line">
        {entries.map((entry) => {
          const value = revealed[entry.id];
          const k = `hist_${entry.id}`;
          return (
            <div key={entry.id} className="grid grid-cols-[1fr_auto] items-center gap-3 bg-bg px-3 py-2">
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <LblText className="text-txt-3">{entry.label}</LblText>
                  <LblText className="text-txt-4">{fmtDate(entry.created)}</LblText>
                </div>
                <div className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-[12px] text-txt">
                  {value ?? "••••••••••••"}
                </div>
              </div>
              <div className="flex gap-1.5">
                <Btn
                  variant="ghost"
                  size="icon"
                  title={value ? "Hide" : "Reveal"}
                  onClick={() => {
                    if (value) {
                      setRevealed((current) => {
                        const next = { ...current };
                        delete next[entry.id];
                        return next;
                      });
                    } else {
                      void reveal(entry).catch((err) =>
                        setError((err as AppError)?.message ?? "Couldn't reveal archived value."),
                      );
                    }
                  }}
                >
                  <Icon name={value ? "eyeoff" : "eye"} size={13} />
                </Btn>
                <Btn
                  variant="ghost"
                  size="icon"
                  title="Copy"
                  onClick={() => {
                    void (async () => {
                      try {
                        const text = value ?? (await reveal(entry));
                        copy(text, k);
                      } catch (err) {
                        setError((err as AppError)?.message ?? "Couldn't copy archived value.");
                      }
                    })();
                  }}
                >
                  <Icon name={copied === k ? "check" : "copy"} size={13} />
                </Btn>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** ServiceMark sourced from a Service's own brand fields. */
function ServiceMarkFromService({ service }: { service: Service }) {
  return (
    <ServiceMark tpl={{ mono: service.mono, color: service.color, slug: service.slug, icon: service.icon }} size={34} />
  );
}
