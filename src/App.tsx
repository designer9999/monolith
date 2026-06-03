/**
 * MONOLITH root: vault lifecycle (status → onboarding/lock/unlocked), view
 * routing, project state, and modals. All data comes from the Rust vault core.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import {
  addService,
  appSettings,
  appPlatform,
  completePairing,
  createProject,
  createVault,
  deleteProject,
  listActivity,
  listItems,
  listProjects,
  lockVault,
  reorderProjects,
  scanPairingQr,
  setClipboardClearMs,
  setProjectIcon,
  storageUsage,
  unlockVault,
  updateAppSettings,
  updateProject,
  vaultStatus,
} from "@/lib/tauri";
import type {
  Activity,
  AppError,
  AppSettings,
  CreateProjectInput,
  Item,
  Project,
  ProjectIcon as ProjectIconData,
  Storage,
  AppPlatform,
  UpdateAppSettingsInput,
  UpdateProjectInput,
} from "@/lib/types";

import { Icon } from "@/lib/icons";
import { Sidebar, TitleBar, type View } from "@/components/Chrome";
import { Home } from "@/components/Home";
import { ProjectView } from "@/components/ProjectView";
import { Browser } from "@/components/Browser";
import { Settings } from "@/components/Settings";
import { CreateProject, EditProject, ServiceCatalog, type ServiceDraft } from "@/components/modals";
import { LockScreen } from "@/components/LockScreen";
import { OnboardingFlow } from "@/components/Onboarding";

type Phase = "loading" | "onboarding" | "locked" | "ready" | "error";

const EMPTY_STORAGE: Storage = { used: "0 KB", total: "∞", pct: 0 };
const DEFAULT_APP_SETTINGS: AppSettings = {
  autoLockMs: 60 * 60 * 1000,
  revealSecretsByDefault: false,
  clipboardClearMs: 30 * 1000,
};

type NavSnapshot = {
  monolith: true;
  view: View;
  activeProjectId: string | null;
  focusId: string | null;
  modal: null | "create" | "catalog" | "editProject";
  catalogProjectId: string | null;
};

export default function App() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [platform, setPlatform] = useState<AppPlatform>("desktop");
  const [bootError, setBootError] = useState<string | null>(null);
  const [view, setView] = useState<View>("home");

  const [projects, setProjects] = useState<Project[]>([]);
  const [items, setItems] = useState<Item[]>([]);
  const [activity, setActivity] = useState<Activity[]>([]);
  const [storage, setStorage] = useState<Storage>(EMPTY_STORAGE);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_APP_SETTINGS);
  const [lockedCount, setLockedCount] = useState(0);
  const [actionError, setActionError] = useState<string | null>(null);

  const [activeProject, setActiveProject] = useState<Project | null>(null);
  const [focusId, setFocusId] = useState<string | null>(null);
  const [modal, setModal] = useState<null | "create" | "catalog" | "editProject">(null);
  const [catalogProject, setCatalogProject] = useState<Project | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const navSyncingRef = useRef(false);
  const lastNavKeyRef = useRef("");

  /** Reload all vault data into state (after unlock or any mutation). */
  const refresh = useCallback(async () => {
    const [p, i, a, s, appPrefs] = await Promise.all([
      listProjects(),
      listItems(),
      listActivity(),
      storageUsage(),
      appSettings(),
    ]);
    setProjects(p);
    setItems(i);
    setActivity(a);
    setStorage(s);
    setSettings(appPrefs);
    setClipboardClearMs(appPrefs.clipboardClearMs);
    setLockedCount(i.length);
    // Keep the open project in sync with fresh data.
    setActiveProject((ap) => (ap ? p.find((x) => x.id === ap.id) ?? null : null));
  }, []);

  /** On boot, decide whether to onboard, lock, or (if already unlocked) load. */
  useEffect(() => {
    void (async () => {
      try {
        const [status, currentPlatform] = await Promise.all([vaultStatus(), appPlatform()]);
        setPlatform(currentPlatform);
        setLockedCount(status.itemCount);
        if (!status.initialized) {
          setPhase("onboarding");
        } else if (status.unlocked) {
          await refresh();
          setPhase("ready");
        } else {
          setPhase("locked");
        }
      } catch (err) {
        // A genuine failure to open the vault is NOT the same as "locked".
        setBootError((err as AppError)?.message ?? "Failed to open the vault.");
        setPhase("error");
      }
    })();
  }, [refresh]);

  const onUnlock = async (password: string) => {
    setActionError(null);
    await unlockVault(password);
    await refresh();
    setPhase("ready");
  };

  const onCreateVault = async (password: string, seedDemo: boolean) => {
    setActionError(null);
    await createVault(password, seedDemo);
    await refresh();
    setPhase("ready");
  };

  const onPairPhone = async () => {
    setActionError(null);
    const qrPayload = await scanPairingQr();
    await completePairing({ qrPayload, deviceName: "Android phone" });
    setModal(null);
    setCatalogProject(null);
    setActiveProject(null);
    setFocusId(null);
    setView("home");
    await refresh();
    setPhase("ready");
  };

  const onLock = useCallback(async () => {
    try {
      setActionError(null);
      const countBeforeLock = items.length || lockedCount;
      await lockVault();
      setLockedCount(countBeforeLock);
      setProjects([]);
      setItems([]);
      setActivity([]);
      setActiveProject(null);
      setView("home");
      setPhase("locked");
    } catch (err) {
      setActionError((err as AppError)?.message ?? "Couldn't lock the vault.");
    }
  }, [items.length, lockedCount]);

  useEffect(() => {
    const autoLockMs = settings.autoLockMs;
    if (phase !== "ready" || autoLockMs == null) return;

    let timer: number | undefined;
    const resetTimer = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        void onLock();
      }, autoLockMs);
    };
    const events = ["pointerdown", "keydown", "touchstart", "mousemove", "focus"] as const;
    events.forEach((event) => window.addEventListener(event, resetTimer, { passive: true }));
    resetTimer();
    return () => {
      window.clearTimeout(timer);
      events.forEach((event) => window.removeEventListener(event, resetTimer));
    };
  }, [phase, settings.autoLockMs, onLock]);

  useEffect(() => {
    if (phase !== "ready") {
      lastNavKeyRef.current = "";
      navSyncingRef.current = false;
    }
  }, [phase]);

  const onUpdateSettings = async (input: UpdateAppSettingsInput) => {
    const saved = await updateAppSettings(input);
    setSettings(saved);
    setClipboardClearMs(saved.clipboardClearMs);
  };

  const navSnapshot = useCallback(
    (): NavSnapshot => ({
      monolith: true,
      view,
      activeProjectId: activeProject?.id ?? null,
      focusId,
      modal,
      catalogProjectId: catalogProject?.id ?? null,
    }),
    [activeProject?.id, catalogProject?.id, focusId, modal, view],
  );

  useEffect(() => {
    if (phase !== "ready" || !(platform === "android" || platform === "ios")) return;
    const snapshot = navSnapshot();
    const key = JSON.stringify(snapshot);
    if (navSyncingRef.current) {
      navSyncingRef.current = false;
      lastNavKeyRef.current = key;
      return;
    }
    if (!lastNavKeyRef.current) {
      window.history.replaceState(snapshot, "");
    } else if (lastNavKeyRef.current !== key) {
      window.history.pushState(snapshot, "");
    }
    lastNavKeyRef.current = key;
  }, [phase, platform, navSnapshot]);

  useEffect(() => {
    if (!(platform === "android" || platform === "ios")) return;
    const restore = (state: unknown) => {
      const next = state as Partial<NavSnapshot> | null;
      if (!next?.monolith) {
        window.history.pushState(navSnapshot(), "");
        return;
      }

      navSyncingRef.current = true;
      const project = next.activeProjectId
        ? projects.find((p) => p.id === next.activeProjectId) ?? null
        : null;
      const catalog = next.catalogProjectId
        ? projects.find((p) => p.id === next.catalogProjectId) ?? null
        : null;

      setActiveProject(project);
      setCatalogProject(catalog);
      setFocusId(next.focusId ?? null);
      setModal(next.modal ?? null);
      setView(project && next.view === "project" ? "project" : next.view ?? "home");
    };
    const onPopState = (event: PopStateEvent) => restore(event.state);
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, [navSnapshot, platform, projects]);

  // --- navigation ---
  const openProject = (p: Project, item?: Item) => {
    setActiveProject(p);
    setFocusId(item ? item.id : null);
    setView("project");
  };
  const newProject = () => setModal("create");
  const editProject = (p: Project) => {
    setActiveProject(p);
    setModal("editProject");
  };
  const addServiceTo = (p: Project | null) => {
    setCatalogProject(p ?? activeProject);
    setModal("catalog");
  };

  // --- mutations ---
  const onCreateProject = async (input: CreateProjectInput) => {
    try {
      setActionError(null);
      const p = await createProject(input);
      setModal(null);
      await refresh();
      openProject(p);
    } catch (err) {
      setActionError((err as AppError)?.message ?? "Couldn't create the project.");
      throw err;
    }
  };

  const onUpdateProject = async (input: UpdateProjectInput) => {
    try {
      setActionError(null);
      const updated = await updateProject(input);
      setModal(null);
      await refresh();
      setActiveProject(updated);
    } catch (err) {
      setActionError((err as AppError)?.message ?? "Couldn't update the project.");
      throw err;
    }
  };

  const onDeleteProject = async (project: Project) => {
    try {
      setActionError(null);
      await deleteProject(project.id);
      if (activeProject?.id === project.id) {
        setActiveProject(null);
        setFocusId(null);
        setView("home");
      }
      if (catalogProject?.id === project.id) {
        setCatalogProject(null);
      }
      await refresh();
    } catch (err) {
      setActionError((err as AppError)?.message ?? "Couldn't delete the project.");
      throw err;
    }
  };

  const onAddService = async (templateId: string, values: ServiceDraft) => {
    if (!catalogProject) return;
    try {
      setActionError(null);
      await addService({ projectId: catalogProject.id, templateId, ...values });
      setModal(null);
      await refresh();
      setReloadKey((k) => k + 1);
    } catch (err) {
      setActionError((err as AppError)?.message ?? "Couldn't add the service.");
      throw err;
    }
  };

  const onSetIcon = async (id: string, icon: ProjectIconData | null) => {
    try {
      setActionError(null);
      await setProjectIcon(id, icon);
      await refresh();
    } catch (err) {
      setActionError((err as AppError)?.message ?? "Couldn't update the project icon.");
    }
  };

  const onReorder = async (orderedIds: string[]) => {
    // Optimistic local reorder, then persist.
    setProjects((prev) => orderedIds.map((id) => prev.find((p) => p.id === id)!).filter(Boolean));
    try {
      setActionError(null);
      await reorderProjects(orderedIds);
      await refresh();
    } catch (err) {
      setActionError((err as AppError)?.message ?? "Couldn't save the project order.");
      await refresh();
    }
  };

  if (phase === "loading") {
    return (
      <div className="grid h-full place-items-center bg-bg font-mono text-[11px] uppercase tracking-[0.18em] text-txt-3">
        Loading vault…
      </div>
    );
  }

  if (phase === "error") {
    return (
      <div className="grid h-full place-items-center bg-bg p-8">
        <div className="max-w-[420px] border border-danger bg-bg-1 p-7 text-center">
          <div className="mb-3 flex justify-center text-danger">
            <Icon name="warn" size={28} />
          </div>
          <div className="mb-2 font-display text-[18px] font-bold">Couldn't open the vault</div>
          <p className="text-[12px] leading-[1.6] text-txt-2">{bootError}</p>
        </div>
      </div>
    );
  }

  if (phase === "onboarding") {
    return (
      <OnboardingFlow
        onComplete={onCreateVault}
        onPairPhone={platform === "android" || platform === "ios" ? onPairPhone : undefined}
      />
    );
  }

  return (
    <div className="relative grid h-full grid-rows-[1fr] isolate pt-[env(safe-area-inset-top)] md:grid-rows-[36px_1fr] md:pt-0">
      {/* faint global grid texture (mask + gradient can't be a static utility) */}
      <div
        aria-hidden
        className="pointer-events-none fixed inset-0 z-0 opacity-[0.35]"
        style={{
          backgroundImage:
            "linear-gradient(var(--line) 1px, transparent 1px), linear-gradient(90deg, var(--line) 1px, transparent 1px)",
          backgroundSize: "64px 64px",
          maskImage: "radial-gradient(ellipse 120% 100% at 50% 0%, #000 30%, transparent 90%)",
        }}
      />

      <TitleBar locked={phase === "locked"} onToggleLock={() => void onLock()} />

      {actionError && (
        <div
          role="alert"
          className="absolute top-12 right-5 z-[50] flex max-w-[420px] items-center gap-2 border border-danger bg-bg-1 px-3 py-2 text-[12px] text-danger shadow-[0_18px_50px_rgba(0,0,0,0.45)]"
        >
          <Icon name="warn" size={14} />
          <span className="min-w-0 flex-1">{actionError}</span>
          <button
            type="button"
            onClick={() => setActionError(null)}
            className="flex text-txt-3 hover:text-danger"
            aria-label="Dismiss error"
          >
            <Icon name="x" size={12} />
          </button>
        </div>
      )}

      <div className="relative z-[1] grid min-h-0 grid-cols-1 pb-[calc(4rem+env(safe-area-inset-bottom))] md:grid-cols-[248px_1fr] md:pb-0">
        <Sidebar
          view={view}
          setView={setView}
          projects={projects}
          onOpenProject={openProject}
          activeProjectId={activeProject?.id}
          onNewProject={newProject}
          onEditProject={editProject}
          onDeleteProject={onDeleteProject}
          onSetIcon={onSetIcon}
          storage={storage}
        />

        <main className="relative min-h-0 overflow-hidden bg-bg-1">
          {view === "home" && (
            <Home
              projects={projects}
              items={items}
              activity={activity}
              onOpenProject={openProject}
              onNewProject={newProject}
              onSetIcon={onSetIcon}
              onEditProject={editProject}
              onDeleteProject={onDeleteProject}
              onReorder={onReorder}
            />
          )}
          {view === "project" && activeProject && (
            <ProjectView
              project={activeProject}
              focusId={focusId}
              onBack={() => setView("home")}
              onAddService={addServiceTo}
              onEditProject={editProject}
              onDeleteProject={onDeleteProject}
              onSetIcon={onSetIcon}
              reloadKey={reloadKey}
              revealSecretsByDefault={settings.revealSecretsByDefault}
            />
          )}
          {view === "browse" && (
            <Browser
              items={items}
              onOpen={(it) => {
                const p = projects.find((x) => x.id === it.projectId);
                if (p) openProject(p, it);
              }}
            />
          )}
          {view === "settings" && (
            <Settings
              items={items}
              projects={projects}
              storage={storage}
              platform={platform}
              settings={settings}
              onSettingsChange={onUpdateSettings}
              onSyncFromDesktop={platform === "android" || platform === "ios" ? onPairPhone : undefined}
              onDataImported={refresh}
              onLock={() => void onLock()}
            />
          )}
        </main>
      </div>

      {modal === "create" && <CreateProject onClose={() => setModal(null)} onCreate={onCreateProject} />}
      {modal === "editProject" && activeProject && (
        <EditProject project={activeProject} onClose={() => setModal(null)} onSave={onUpdateProject} />
      )}
      {modal === "catalog" && (
        <ServiceCatalog project={catalogProject} onClose={() => setModal(null)} onAdd={onAddService} />
      )}
      {phase === "locked" && (
        <LockScreen onUnlock={onUnlock} count={lockedCount} />
      )}
    </div>
  );
}
