/**
 * Typed wrappers over the Tauri command surface. This is the single bridge
 * between the React UI and the Rust vault core — no component calls `invoke`
 * directly. Argument keys are camelCase; Tauri maps them to Rust snake_case.
 */

import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  writeText,
  clear as clearClipboard,
} from "@tauri-apps/plugin-clipboard-manager";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent } from "@tauri-apps/plugin-updater";

import type {
  Activity,
  AddServiceInput,
  AgentImportBundle,
  AgentImportResult,
  AppSettings,
  AppPlatform,
  CompletePairingInput,
  CreateProjectInput,
  Item,
  PairedDevice,
  PasswordHistoryEntry,
  PairingImportResult,
  PairingSession,
  PairingSessionStatus,
  Project,
  ProjectIcon,
  RevealedSecret,
  Service,
  Storage,
  Template,
  TotpCode,
  UpdateAppSettingsInput,
  UpdateProjectInput,
  UpdateServiceInput,
  VaultStatus,
} from "./types";

/** Whether the app is running inside the Tauri webview (vs. a plain browser). */
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function cmd<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

// --- vault lifecycle ---

export const vaultStatus = () => cmd<VaultStatus>("vault_status");
export const appPlatform = () => cmd<AppPlatform>("app_platform");
export const createVault = (masterPassword: string, seedDemo: boolean) =>
  cmd<VaultStatus>("create_vault", { masterPassword, seedDemo });
export const unlockVault = (masterPassword: string) =>
  cmd<VaultStatus>("unlock_vault", { masterPassword });
export const restoreRememberedUnlock = () => cmd<VaultStatus>("restore_remembered_unlock");
export const lockVault = () => cmd<void>("lock_vault");

// --- reads ---

export const listProjects = () => cmd<Project[]>("list_projects");
export const listServices = (projectId: string) =>
  cmd<Service[]>("list_services", { projectId });
export const listItems = () => cmd<Item[]>("list_items");
export const listActivity = () => cmd<Activity[]>("list_activity");
export const listTemplates = () => cmd<Template[]>("list_templates");
export const storageUsage = () => cmd<Storage>("storage_usage");
export const appSettings = () => cmd<AppSettings>("app_settings");

// --- mutations ---

export const createProject = (input: CreateProjectInput) =>
  cmd<Project>("create_project", { input });
export const updateProject = (input: UpdateProjectInput) =>
  cmd<Project>("update_project", { input });
export const setProjectIcon = (projectId: string, icon: ProjectIcon | null) =>
  cmd<void>("set_project_icon", { projectId, icon });
export const deleteProject = (projectId: string) =>
  cmd<void>("delete_project", { projectId });
export const reorderProjects = (orderedIds: string[]) =>
  cmd<void>("reorder_projects", { orderedIds });
export const addService = (input: AddServiceInput) => cmd<string>("add_service", { input });
export const importAgentBundle = (bundle: AgentImportBundle) =>
  cmd<AgentImportResult>("import_agent_bundle", { bundle });
export const updateService = (input: UpdateServiceInput) =>
  cmd<Service>("update_service", { input });
export const deleteService = (serviceId: string) => cmd<void>("delete_service", { serviceId });
export const revealField = (fieldId: string) =>
  cmd<RevealedSecret>("reveal_field", { fieldId });
export const listPasswordHistory = (serviceId: string) =>
  cmd<PasswordHistoryEntry[]>("list_password_history", { serviceId });
export const revealHistory = (historyId: string) =>
  cmd<RevealedSecret>("reveal_history", { historyId });
export const generateTotp = (serviceId: string) =>
  cmd<TotpCode>("generate_totp", { serviceId });
export const addAttachment = (projectId: string, name: string, size: string) =>
  cmd<import("./types").Attachment>("add_attachment", { projectId, name, size });
export const updateAppSettings = (input: UpdateAppSettingsInput) =>
  cmd<AppSettings>("update_app_settings", { input });

// --- local QR pairing ---

export const startPairingSession = () => cmd<PairingSession>("start_pairing_session");
export const pairingSessionStatus = (sessionId: string) =>
  cmd<PairingSessionStatus>("pairing_session_status", { sessionId });
export const cancelPairingSession = (sessionId: string) =>
  cmd<void>("cancel_pairing_session", { sessionId });
export const approvePairingSession = (sessionId: string) =>
  cmd<PairingSessionStatus>("approve_pairing_session", { sessionId });
export const listDevices = () => cmd<PairedDevice[]>("list_devices");
export const revokeDevice = (deviceId: string) => cmd<void>("revoke_device", { deviceId });
export const completePairing = (input: CompletePairingInput) =>
  cmd<PairingImportResult>("complete_pairing", { input });
export const unlockDeviceVault = (deviceKey: string) =>
  cmd<VaultStatus>("unlock_device_vault", { deviceKey });

export async function scanPairingQr(): Promise<string> {
  const scanner = await import("@tauri-apps/plugin-barcode-scanner");
  const permission = await scanner.checkPermissions();
  if (permission !== "granted") {
    const requested = await scanner.requestPermissions();
    if (requested !== "granted") {
      throw new Error("Camera permission is required to scan a pairing QR code.");
    }
  }
  const result = await scanner.scan({
    cameraDirection: "back",
    formats: [scanner.Format.QRCode],
  });
  return result.content;
}

// --- window controls (frameless titlebar) ---

const appWindow = isTauri ? getCurrentWindow() : null;
export const winMinimize = () => appWindow?.minimize();
export const winToggleMaximize = () => appWindow?.toggleMaximize();
export const winClose = () => appWindow?.close();

// --- signed desktop updater ---

export type AppUpdateProgress =
  | { phase: "checking" }
  | { phase: "none" }
  | { phase: "found"; version: string; currentVersion: string; notes?: string }
  | { phase: "downloading"; downloaded: number; total?: number }
  | { phase: "installing" }
  | { phase: "relaunching" };

export interface AppUpdateResult {
  updated: boolean;
  version?: string;
  currentVersion?: string;
}

export async function checkInstallAndRelaunch(
  onProgress: (progress: AppUpdateProgress) => void,
): Promise<AppUpdateResult> {
  onProgress({ phase: "checking" });
  const update = await check({ timeout: 30_000 });
  if (!update) {
    onProgress({ phase: "none" });
    return { updated: false };
  }

  onProgress({
    phase: "found",
    version: update.version,
    currentVersion: update.currentVersion,
    notes: update.body,
  });

  let downloaded = 0;
  let total: number | undefined;
  await update.downloadAndInstall((event: DownloadEvent) => {
    if (event.event === "Started") {
      downloaded = 0;
      total = event.data.contentLength;
      onProgress({ phase: "downloading", downloaded, total });
    } else if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      onProgress({ phase: "downloading", downloaded, total });
    } else if (event.event === "Finished") {
      onProgress({ phase: "installing" });
    }
  });

  onProgress({ phase: "relaunching" });
  await relaunch();
  return { updated: true, version: update.version, currentVersion: update.currentVersion };
}

// --- clipboard with auto-clear ---

/** How long (ms) a copied secret stays on the clipboard before it's auto-cleared. */
let clipboardClearMs = 30_000;

/** Update the clipboard auto-clear delay (used by the Settings control). */
export function setClipboardClearMs(ms: number): void {
  if (![10_000, 30_000, 60_000].includes(ms)) return;
  clipboardClearMs = ms;
}

/**
 * Copy text to the OS clipboard, then clear it after the configured delay.
 * The timer intentionally does not retain the copied text for comparison; this
 * avoids clipboard-read permission and keeps secret lifetime shorter in JS.
 */
export async function copyWithAutoClear(text: string): Promise<void> {
  if (isTauri) {
    await writeText(text);
    if (clipboardClearMs > 0) {
      window.setTimeout(async () => {
        try {
          await clearClipboard();
        } catch {
          /* clipboard may be unavailable; ignore */
        }
      }, clipboardClearMs);
    }
    return;
  }
  // Browser fallback (dev only).
  try {
    await navigator.clipboard?.writeText(text);
  } catch {
    /* ignore */
  }
}
