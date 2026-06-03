/**
 * Settings: system configuration mirroring the Tauri/Rust vault architecture.
 * Ported 1:1 from the design's settings.jsx. Encryption / security / Tauri
 * capabilities / storage / backup / mobile companion / about + danger zone.
 * Per-project storage bars use real project colors (inline, data-driven).
 * Unimplemented settings are visibly disabled.
 */

import { useEffect, useRef, useState, type ChangeEvent, type DragEvent } from "react";
import { QRCodeSVG } from "qrcode.react";

import type {
  AgentBridgeSession,
  AgentImportBundle,
  AgentImportResult,
  AppError,
  AppPlatform,
  AppSettings,
  Item,
  PairedDevice,
  PairingSession,
  Project,
  Storage,
  UpdateAppSettingsInput,
} from "@/lib/types";
import {
  approvePairingSession,
  agentBridgeStatus,
  cancelPairingSession,
  checkInstallAndRelaunch,
  copyText,
  importAgentBundle,
  importAgentBundleFile,
  listDevices,
  pairingSessionStatus,
  revokeDevice,
  startAgentBridge,
  startPairingSession,
  stopAgentBridge,
  type AppUpdateProgress,
} from "@/lib/tauri";
import { Icon } from "@/lib/icons";
import { cn } from "@/lib/utils";
import { Btn } from "@/components/ui/btn";
import { Chip, Lbl, LblText, SectionHead } from "@/components/ui/primitives";

/** Sliding on/off switch (the design's Toggle). Disabled = a planned/unwired setting. */
function Toggle({ on, onClick, disabled }: { on: boolean; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      role="switch"
      aria-checked={on}
      className={cn(
        "flex h-[22px] w-[42px] items-center border p-0.5 transition-all duration-150",
        on ? "justify-end border-acc bg-acc" : "justify-start border-line-2 bg-bg-3",
        disabled ? "cursor-not-allowed opacity-40" : "cursor-pointer",
      )}
    >
      <span className={cn("block size-[14px]", on ? "bg-acc-ink" : "bg-txt-3")} />
    </button>
  );
}

/** A labelled config row with optional description, divided by a hairline. */
function SettingRow({
  label,
  desc,
  danger,
  children,
}: {
  label: string;
  desc?: string;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-start justify-between gap-3 border-b border-line py-[15px] sm:flex-row sm:items-center sm:gap-6">
      <div className="min-w-0">
        <div className={cn("mb-1 text-[12.5px]", danger ? "text-danger" : "text-txt")}>{label}</div>
        {desc && <div className="text-[11px] leading-[1.5] text-txt-3">{desc}</div>}
      </div>
      <div className="w-full flex-none sm:w-auto">{children}</div>
    </div>
  );
}

/** Horizontal segmented selector (the design's Segmented). */
function Segmented({
  value,
  options,
  onChange,
  disabled,
}: {
  value: string;
  options: string[];
  onChange: (o: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex w-full overflow-x-auto border border-line-2 sm:w-auto">
      {options.map((o, i) => (
        <button
          key={o}
          disabled={disabled}
          onClick={() => onChange(o)}
          className={cn(
            "cursor-pointer px-[13px] py-[7px] text-[10px] tracking-[0.1em] uppercase disabled:cursor-not-allowed disabled:opacity-40",
            i ? "border-l border-line" : "",
            value === o ? "bg-bg-3 text-acc" : "bg-transparent text-txt-3",
          )}
        >
          {o}
        </button>
      ))}
    </div>
  );
}

/** Key / value cell on a hairline-gap grid. */
function KV({ k, v }: { k: string; v: string }) {
  return (
    <div className="bg-bg-1 px-[14px] py-3">
      <Lbl className="mb-1.5 text-txt-4">{k}</Lbl>
      <div className="font-mono text-[11.5px] text-txt">{v}</div>
    </div>
  );
}

function autoLockLabel(ms?: number): string {
  if (ms == null) return "Never";
  if (ms === 60 * 60 * 1000) return "1 h";
  if (ms === 24 * 60 * 60 * 1000) return "24 h";
  if (ms === 7 * 24 * 60 * 60 * 1000) return "7 days";
  return "1 h";
}

function autoLockMsFromLabel(label: string): number | null {
  if (label === "24 h") return 24 * 60 * 60 * 1000;
  if (label === "7 days") return 7 * 24 * 60 * 60 * 1000;
  if (label === "Never") return null;
  return 60 * 60 * 1000;
}

function clipboardLabel(ms: number): string {
  if (ms === 10 * 1000) return "10 s";
  if (ms === 60 * 1000) return "60 s";
  return "30 s";
}

function clipboardMsFromLabel(label: string): number {
  if (label === "10 s") return 10 * 1000;
  if (label === "60 s") return 60 * 1000;
  return 30 * 1000;
}

function updateProgressText(progress: AppUpdateProgress | null): string {
  if (!progress) return "READY";
  if (progress.phase === "checking") return "CHECKING";
  if (progress.phase === "none") return "UP TO DATE";
  if (progress.phase === "found") return `FOUND ${progress.version}`;
  if (progress.phase === "installing") return "INSTALLING";
  if (progress.phase === "relaunching") return "RELAUNCHING";
  const total = progress.total;
  if (!total) return `${Math.round(progress.downloaded / 1024)} KB`;
  return `${Math.min(100, Math.round((progress.downloaded / total) * 100))}%`;
}

const MAX_IMPORT_FILE_BYTES = 8 * 1024 * 1024;

const AGENT_TEMPLATE_GUIDE = `Supported templateId values and useful field labels:
- github: Username, Account Email, Personal Access Token, SSH Private Key, Webhook Secret, OAuth Client ID, OAuth Secret
- apple: Account Email, Password, Recovery Email, Trusted Phone, Recovery Key, Backup Codes
- mega: Account Email, Password, Recovery Key, Notes
- topaz: Account Email, Password, License Key, Notes
- huggingface: Username, Account Email, Access Token, Organization
- instagram: Username, Account Email, Password, Recovery Email, Phone, Backup Codes
- login: URL, Email / Username, Password
- zeroid: Client ID, Client Secret, Issuer URL, Account Email
- openai: API Key, Organization ID, Project ID
- vercel: Account Email, Access Token, Team ID, Project ID, Deploy Hook URL
- supabase: Project URL, Anon Key, Service Role Key, JWT Secret, Database Password, S3 Access Key, S3 Secret Key
- postgres: Host, Port, User, Password, Database, Connection URL
- ssh: Host, User, Private Key, Passphrase
- domain: Registrar, Login Email, Password, EPP / Auth Code, Renewal Date
- card: Card Number, Expiry, CVV, Cardholder
- note: Note
- also available: google, stripe, cloudflare, aws, shopify, smtp, prisma, claude, resend, runpod`;

const AGENT_IMPORT_PROMPT = `You are preparing a MONOLITH agent import bundle.

Read only these local credential folders or files:
- <paste credential folder path 1>
- <paste credential folder path 2>

Do not print, summarize, or expose secret values in chat. Produce one JSON file that matches docs/agent-import.schema.json. Use version 1.

Put global and personal accounts under defaultProjectName "Personal". Put project-specific credentials under projectName only when the file clearly names a project.

Use stable labels, because MONOLITH upserts by project + templateId + label. Re-running the same import should update existing services, not create duplicates.

${AGENT_TEMPLATE_GUIDE}

Use note for anything that does not fit a template yet.

Use expiresAt only when a real expiration, renewal, or planned rotation date is present, formatted YYYY-MM-DD. Do not invent dates.

Save the result as monolith-import.monolith-import.json. Do not commit the file. After import, delete the plaintext bundle.`;

function buildAgentPrompt(bridge: AgentBridgeSession | null): string {
  if (!bridge) return AGENT_IMPORT_PROMPT;
  return `You are importing credentials into MONOLITH through its local agent bridge.

Read only the credential folders or files the user explicitly names. Do not print, summarize, or expose secret values in chat.

First fetch the live MONOLITH capabilities. They include the exact templates, field labels, JSON schema, size limits, and an example bundle:
GET ${bridge.capabilitiesUrl}
Header: X-MONOLITH-Agent-Token: ${bridge.token}

Then POST a JSON bundle directly into MONOLITH:
POST ${bridge.importUrl}
Header: X-MONOLITH-Agent-Token: ${bridge.token}
Content-Type: application/json

Bundle root shape:
{"version":1,"source":"local credential folders","defaultProjectName":"Personal","items":[...]}

Rules:
- Put global and personal accounts under defaultProjectName "Personal".
- Put project-specific credentials under projectName only when a file clearly names a project.
- Use stable labels because MONOLITH upserts by project + templateId + label.
- Use expiresAt only for real expiration, renewal, or rotation dates in YYYY-MM-DD format.
- Use note for anything that does not fit a template yet.
- Never invent missing secret values.

${AGENT_TEMPLATE_GUIDE}

This bridge is loopback-only, write-only, and expires at ${bridge.expiresAt}.`;
}

export function Settings({
  items,
  projects,
  storage,
  platform,
  settings,
  onSettingsChange,
  onSyncFromDesktop,
  onDataImported,
  onLock,
}: {
  items: Item[];
  projects: Project[];
  storage: Storage;
  platform: AppPlatform;
  settings: AppSettings;
  onSettingsChange: (input: UpdateAppSettingsInput) => Promise<void>;
  onSyncFromDesktop?: () => Promise<void>;
  onDataImported: () => Promise<void>;
  onLock: () => void;
}) {
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [bio, setBio] = useState(false);
  const [keychain, setKeychain] = useState(false);
  const [tele, setTele] = useState(false);
  const [breach, setBreach] = useState(false);
  const [devices, setDevices] = useState<PairedDevice[]>([]);
  const [pairing, setPairing] = useState<PairingSession | null>(null);
  const [pairingBusy, setPairingBusy] = useState(false);
  const [pairingError, setPairingError] = useState<string | null>(null);
  const [syncBusy, setSyncBusy] = useState(false);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncOk, setSyncOk] = useState<string | null>(null);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<AppUpdateProgress | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [importText, setImportText] = useState("");
  const [importBusy, setImportBusy] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [importResult, setImportResult] = useState<AgentImportResult | null>(null);
  const [importDropActive, setImportDropActive] = useState(false);
  const [importPromptCopied, setImportPromptCopied] = useState(false);
  const [bridge, setBridge] = useState<AgentBridgeSession | null>(null);
  const [bridgeBusy, setBridgeBusy] = useState(false);
  const [bridgeError, setBridgeError] = useState<string | null>(null);
  const autoApprovingRef = useRef(false);
  const importFileRef = useRef<HTMLInputElement>(null);
  const isMobile = platform === "android" || platform === "ios";
  const autolock = autoLockLabel(settings.autoLockMs);
  const clip = clipboardLabel(settings.clipboardClearMs);
  const agentPrompt = buildAgentPrompt(bridge);

  const counts: Record<string, number> = {};
  items.forEach((i) => {
    counts[i.projectId] = (counts[i.projectId] || 0) + 1;
  });
  const sizes = projects.map((p) => {
    const count = counts[p.id] || 0;
    return { id: p.id, name: p.name, color: p.color, count, kb: count * 42 + 18 };
  });
  const totalKb = sizes.reduce((a, b) => a + b.kb, 0) || 1;

  const caps: [string, boolean][] = [
    ["clipboard-manager · write / clear", true],
    ["clipboard · read", false],
    ["window · controls (frameless)", true],
    ["dialog · open / save", false],
    ["shell · execute", false],
    ["fs · arbitrary read / write", false],
    ["github api · token autofill", true],
    ["agent bridge · loopback import only", true],
    ["updater · signed GitHub releases", !isMobile],
    ["barcode scanner · mobile only", true],
  ];

  const phases: [string, string, string][] = [
    ["1", "Local encrypted vault", "READY"],
    ["2", "Android QR pairing", "READY"],
    ["3", "Local encrypted transfer", "READY"],
    ["4", "Cloud sync", "PLANNED"],
  ];

  const refreshDevices = async () => {
    try {
      setDevices(await listDevices());
    } catch {
      setDevices([]);
    }
  };

  const refreshBridge = async () => {
    try {
      setBridge(await agentBridgeStatus());
    } catch {
      setBridge(null);
    }
  };

  useEffect(() => {
    void refreshDevices();
    void refreshBridge();
  }, []);

  useEffect(() => {
    if (!bridge) return;
    const timer = window.setInterval(() => {
      void refreshBridge();
    }, 15_000);
    return () => window.clearInterval(timer);
  }, [bridge]);

  const startBridge = async () => {
    if (bridgeBusy) return;
    setBridgeBusy(true);
    setBridgeError(null);
    try {
      setBridge(await startAgentBridge());
    } catch (err) {
      setBridgeError((err as AppError)?.message ?? "Could not start the local agent bridge.");
    } finally {
      setBridgeBusy(false);
    }
  };

  const stopBridge = async () => {
    if (bridgeBusy) return;
    setBridgeBusy(true);
    setBridgeError(null);
    try {
      await stopAgentBridge();
      setBridge(null);
    } catch (err) {
      setBridgeError((err as AppError)?.message ?? "Could not stop the local agent bridge.");
    } finally {
      setBridgeBusy(false);
    }
  };

  useEffect(() => {
    if (!pairing || pairing.approved) return;
    const timer = window.setInterval(() => {
      void (async () => {
        try {
          const status = await pairingSessionStatus(pairing.id);
          setPairing((current) =>
            current
              ? {
                  ...current,
                  approved: status.approved,
                  pendingDeviceName: status.pendingDeviceName,
                }
              : current,
          );
          if (status.pendingDeviceName && !status.approved && !autoApprovingRef.current) {
            autoApprovingRef.current = true;
            try {
              const approved = await approvePairingSession(pairing.id);
              setPairing((current) =>
                current
                  ? {
                      ...current,
                      approved: approved.approved,
                      pendingDeviceName: approved.pendingDeviceName ?? status.pendingDeviceName,
                    }
                  : current,
              );
              await refreshDevices();
            } finally {
              autoApprovingRef.current = false;
            }
          }
          if (status.expired) {
            setPairing(null);
            setPairingError("Pairing session expired. Start a new QR session.");
          }
        } catch (err) {
          setPairingError((err as AppError)?.message ?? "Could not read pairing status.");
        }
      })();
    }, 1000);
    return () => window.clearInterval(timer);
  }, [pairing]);

  const startPairing = async () => {
    setPairingBusy(true);
    setPairingError(null);
    try {
      setPairing(await startPairingSession());
    } catch (err) {
      setPairingError((err as AppError)?.message ?? "Could not start pairing.");
    } finally {
      setPairingBusy(false);
    }
  };

  const cancelPairing = async () => {
    if (!pairing) return;
    setPairingBusy(true);
    try {
      await cancelPairingSession(pairing.id);
      setPairing(null);
    } catch (err) {
      setPairingError((err as AppError)?.message ?? "Could not cancel pairing.");
    } finally {
      setPairingBusy(false);
    }
  };

  const approvePairing = async () => {
    if (!pairing) return;
    setPairingBusy(true);
    setPairingError(null);
    try {
      const status = await approvePairingSession(pairing.id);
      setPairing((current) => (current ? { ...current, approved: status.approved } : current));
      await refreshDevices();
    } catch (err) {
      setPairingError((err as AppError)?.message ?? "Could not approve this phone.");
    } finally {
      setPairingBusy(false);
    }
  };

  const syncFromDesktop = async () => {
    if (!onSyncFromDesktop || syncBusy) return;
    setSyncBusy(true);
    setSyncError(null);
    setSyncOk(null);
    try {
      await onSyncFromDesktop();
      setSyncOk("Vault updated from desktop.");
    } catch (err) {
      setSyncError((err as AppError)?.message ?? "Could not sync from desktop.");
    } finally {
      setSyncBusy(false);
    }
  };

  const revoke = async (deviceId: string) => {
    setPairingError(null);
    try {
      await revokeDevice(deviceId);
      await refreshDevices();
    } catch (err) {
      setPairingError((err as AppError)?.message ?? "Could not revoke device.");
    }
  };

  const saveSettings = async (patch: Partial<UpdateAppSettingsInput>) => {
    if (settingsBusy) return;
    setSettingsBusy(true);
    setSettingsError(null);
    try {
      await onSettingsChange({
        autoLockMs: settings.autoLockMs ?? null,
        revealSecretsByDefault: settings.revealSecretsByDefault,
        clipboardClearMs: settings.clipboardClearMs,
        ...patch,
      });
    } catch (err) {
      setSettingsError((err as AppError)?.message ?? "Could not update settings.");
    } finally {
      setSettingsBusy(false);
    }
  };

  const runUpdater = async () => {
    if (isMobile || updateBusy) return;
    setUpdateBusy(true);
    setUpdateError(null);
    try {
      await checkInstallAndRelaunch(setUpdateProgress);
    } catch (err) {
      setUpdateError((err as AppError)?.message ?? "Could not update MONOLITH.");
    } finally {
      setUpdateBusy(false);
    }
  };

  const finishAgentImport = async (result: AgentImportResult) => {
    setImportResult(result);
    if (result.created + result.updated > 0) {
      await onDataImported();
    }
  };

  const runAgentImportText = async (text: string) => {
    if (importBusy) return;
    const trimmed = text.trim();
    if (!trimmed) return;
    setImportBusy(true);
    setImportError(null);
    setImportResult(null);
    try {
      const parsed = JSON.parse(trimmed) as AgentImportBundle;
      if (!parsed || !Array.isArray(parsed.items)) {
        throw new Error("Import JSON must include an items array.");
      }
      const result = await importAgentBundle(parsed);
      await finishAgentImport(result);
    } catch (err) {
      setImportError((err as AppError)?.message ?? (err as Error)?.message ?? "Import failed.");
    } finally {
      setImportBusy(false);
    }
  };

  const runAgentImportFilePath = async (path: string) => {
    if (importBusy) return;
    setImportBusy(true);
    setImportError(null);
    setImportResult(null);
    try {
      const result = await importAgentBundleFile(path);
      await finishAgentImport(result);
    } catch (err) {
      setImportError((err as AppError)?.message ?? (err as Error)?.message ?? "Import failed.");
    } finally {
      setImportBusy(false);
    }
  };

  const runAgentImport = async () => {
    await runAgentImportText(importText);
  };

  const importFile = async (file: File) => {
    setImportError(null);
    setImportResult(null);
    if (file.size > MAX_IMPORT_FILE_BYTES) {
      setImportError("Import bundle is too large for local import.");
      return;
    }
    try {
      const text = await file.text();
      setImportText(text);
      await runAgentImportText(text);
    } catch (err) {
      const path = (file as File & { path?: string }).path;
      if (path) {
        await runAgentImportFilePath(path);
        return;
      }
      setImportError((err as AppError)?.message ?? (err as Error)?.message ?? "Could not read dropped file.");
    }
  };

  const onImportDrop = async (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setImportDropActive(false);
    const file = event.dataTransfer.files.item(0);
    if (file) {
      await importFile(file);
      return;
    }
    const text = event.dataTransfer.getData("text/plain").trim();
    if (text) {
      setImportText(text);
      await runAgentImportText(text);
      return;
    }
    setImportError("Drop a MONOLITH JSON bundle file or JSON text.");
  };

  const onImportFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.item(0);
    event.currentTarget.value = "";
    if (file) {
      await importFile(file);
    }
  };

  const copyAgentPrompt = async () => {
    try {
      await copyText(agentPrompt);
      setImportPromptCopied(true);
      window.setTimeout(() => setImportPromptCopied(false), 1400);
    } catch (err) {
      setImportError((err as AppError)?.message ?? (err as Error)?.message ?? "Could not copy prompt.");
    }
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="border-b border-line px-4 pt-5 pb-4 sm:px-[30px] sm:pt-[26px] sm:pb-5">
        <Lbl className="mb-2">SYSTEM · CONFIGURATION</Lbl>
        <h1 className="font-display text-[24px] font-bold sm:text-[28px]">Settings</h1>
      </div>

      <div className="flex max-w-[880px] flex-col gap-8 px-4 pt-5 pb-[72px] sm:gap-9 sm:px-[30px] sm:pt-6 sm:pb-[60px]">
        {/* ENCRYPTION */}
        <section>
          <SectionHead icon="shield" title="Encryption" right="ENVELOPE · ARGON2ID" />
          <div className="mb-3.5 grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-px bg-line">
            <KV k="Key derivation" v="Argon2id" />
            <KV k="KDF cost" v="64 MiB · t=3 · p=1" />
            <KV k="Cipher" v="XChaCha20-Poly1305" />
            <KV k="Database" v="SQLite + per-field enc" />
          </div>
          <SettingRow
            label="Whole-database encryption (SQLCipher)"
            desc="Planned Stage 2 — would encrypt metadata and the DB file on top of per-field encryption. Not active yet; secrets are already protected by per-field XChaCha20-Poly1305."
          >
            <Chip tone="default">PLANNED</Chip>
          </SettingRow>
          <SettingRow
            label="Change master password"
            desc="Would re-derive your wrapping key and re-wrap (not re-encrypt) the vault key. Not implemented yet."
          >
            <div className="flex items-center gap-2.5">
              <Chip tone="default">PLANNED</Chip>
              <Btn variant="ghost" disabled>
                <Icon name="key" size={13} /> Change
              </Btn>
            </div>
          </SettingRow>
        </section>

        {/* SECURITY */}
        <section>
          <SectionHead icon="key" title="Security" />
          <SettingRow
            label="Clear clipboard"
            desc="Copied secrets are wiped from the OS clipboard automatically after this delay. Active."
          >
            <Segmented
              value={clip}
              options={["10 s", "30 s", "60 s"]}
              onChange={(v) => void saveSettings({ clipboardClearMs: clipboardMsFromLabel(v) })}
              disabled={settingsBusy}
            />
          </SettingRow>
          <SettingRow
            label="Show secrets by default"
            desc="Reveal credential values automatically when a service panel is open. Off is safer on shared screens."
          >
            <Toggle
              on={settings.revealSecretsByDefault}
              onClick={() => void saveSettings({ revealSecretsByDefault: !settings.revealSecretsByDefault })}
              disabled={settingsBusy}
            />
          </SettingRow>
          <SettingRow
            label="Auto-lock session"
            desc="Lock after inactivity and expire the remembered local unlock session. Never keeps this Windows login trusted until you lock manually."
          >
            <div className="flex items-center gap-2.5">
              <Segmented
                value={autolock}
                options={["1 h", "24 h", "7 days", "Never"]}
                onChange={(v) => void saveSettings({ autoLockMs: autoLockMsFromLabel(v) })}
                disabled={settingsBusy}
              />
            </div>
          </SettingRow>
          {settingsError && (
            <div className="-mt-2 border border-danger bg-bg px-3 py-2 text-[11px] text-danger">
              {settingsError}
            </div>
          )}
          <SettingRow
            label="Windows Hello unlock"
            desc="Biometric convenience unlock after a master-password session. Planned — not implemented."
          >
            <div className="flex items-center gap-2.5">
              <Chip tone="default">PLANNED</Chip>
              <Toggle on={bio} onClick={() => setBio((b) => !b)} disabled />
            </div>
          </SettingRow>
          <SettingRow
            label="Store key in OS keychain"
            desc="Would change your threat model — anyone with your OS account could unlock. Planned — not implemented; off by design."
          >
            <div className="flex items-center gap-2.5">
              <Chip tone="default">PLANNED</Chip>
              <Toggle on={keychain} onClick={() => setKeychain((k) => !k)} disabled />
            </div>
          </SettingRow>
          <SettingRow
            label="Breach monitoring"
            desc="Check secrets against a local, offline leak database. Planned — not implemented."
          >
            <div className="flex items-center gap-2.5">
              <Chip tone="default">PLANNED</Chip>
              <Toggle on={breach} onClick={() => setBreach((b) => !b)} disabled />
            </div>
          </SettingRow>
        </section>

        {/* CAPABILITIES */}
        <section>
          <SectionHead icon="layers" title="App capabilities" right="TAURI · LEAST PRIVILEGE" />
          <div className="grid grid-cols-1 gap-px bg-line sm:grid-cols-2">
            {caps.map(([c, allow]) => (
              <div key={c} className="flex items-center gap-2.5 bg-bg-1 px-[14px] py-[11px]">
                <span className={cn("flex", allow ? "text-ok" : "text-txt-4")}>
                  <Icon name={allow ? "check" : "x"} size={13} />
                </span>
                <span className={cn("flex-1 font-mono text-[11px]", allow ? "text-txt-2" : "text-txt-4")}>
                  {c}
                </span>
                <LblText className={allow ? "text-ok" : "text-txt-4"}>{allow ? "ALLOW" : "DENY"}</LblText>
              </div>
            ))}
          </div>
          <SettingRow
            label="Strict Content-Security-Policy"
            desc="No remote scripts. connect-src is limited to local IPC and GitHub API token autofill. Inline styles are allowed for data-driven UI colors."
          >
            <Chip tone="accent">RESTRICTED</Chip>
          </SettingRow>
          <SettingRow
            label="Anonymous diagnostics"
            desc="Off by design — there is no telemetry. Never includes secret data."
          >
            <div className="flex items-center gap-2.5">
              <Chip tone="default">PLANNED</Chip>
              <Toggle on={tele} onClick={() => setTele((t) => !t)} disabled />
            </div>
          </SettingRow>
        </section>

        {/* STORAGE */}
        <section>
          <SectionHead icon="vault" title="Storage" right={`${storage.used} · ${storage.total}`} />
          <div className="mb-3 flex h-3 border border-line-2 bg-bg">
            {sizes.map((s, i) => (
              <div
                key={s.id}
                title={`${s.name} · ${s.kb}KB content estimate`}
                className={cn("opacity-85", i < sizes.length - 1 && "border-r border-bg")}
                style={{ width: `${(s.kb / totalKb) * 100}%`, background: s.color }}
              />
            ))}
          </div>
          <div className="grid grid-cols-1 gap-px bg-line min-[430px]:grid-cols-2 sm:grid-cols-[repeat(auto-fill,minmax(150px,1fr))]">
            {sizes.map((s) => (
              <div key={s.id} className="bg-bg-1 px-[14px] py-3">
                <div className="mb-2 flex items-center gap-2">
                  <span className="size-2" style={{ background: s.color }} />
                  <LblText className="text-txt-2">{s.name}</LblText>
                </div>
                <div className="font-display tabular-nums text-[18px] font-semibold">
                  {s.kb} <span className="text-[11px] text-txt-3">KB</span>
                </div>
                <Lbl className="mt-1 text-txt-4">{s.count} SERVICES · EST.</Lbl>
              </div>
            ))}
          </div>
        </section>

        {/* BACKUP */}
        <section>
          <SectionHead icon="dl" title="Backup & recovery" right="PLANNED" />
          <SettingRow
            label="Vault file"
            desc="Stored in the OS app-data directory (monolith.vault.db). Secret fields and TOTP seeds are encrypted with the vault key; the SQLite file itself is not yet whole-DB encrypted."
          >
            <Chip tone="default">PLANNED</Chip>
          </SettingRow>
          <SettingRow
            label="Export encrypted backup"
            desc="Would export ciphertext only — unreadable without your master password. Not implemented yet."
          >
            <div className="flex items-center gap-2.5">
              <Chip tone="default">PLANNED</Chip>
              <Btn disabled>
                <Icon name="dl" size={13} /> Export
              </Btn>
            </div>
          </SettingRow>
          <SettingRow label="Import backup" desc="Restore from an encrypted export. Not implemented yet.">
            <div className="flex items-center gap-2.5">
              <Chip tone="default">PLANNED</Chip>
              <Btn variant="ghost" disabled>
                <Icon name="upload" size={13} /> Import
              </Btn>
            </div>
          </SettingRow>
        </section>

        {/* MOBILE */}
        <section>
          <SectionHead icon="refresh" title="Devices & pairing" right="ANDROID · LOCAL" />
          {isMobile && (
            <SettingRow
              label="Local sync from desktop"
              desc="Scan a desktop QR to pull the latest encrypted vault snapshot onto this phone."
            >
              <div className="flex flex-col gap-2 sm:items-end">
                <Btn onClick={() => void syncFromDesktop()} disabled={syncBusy}>
                  <Icon name="qr" size={13} /> {syncBusy ? "Syncing..." : "Scan desktop QR"}
                </Btn>
                {syncError && <div className="text-[11px] text-danger">{syncError}</div>}
                {syncOk && <div className="text-[11px] text-ok">{syncOk}</div>}
              </div>
            </SettingRow>
          )}
          <div className="grid grid-cols-1 items-start gap-4 border border-line bg-bg-1 p-4 sm:grid-cols-[auto_1fr] sm:gap-6 sm:p-[18px]">
            <div className="text-center">
              <div className="mx-auto grid size-[156px] place-items-center border border-line-2 bg-bg p-3">
                {pairing ? (
                  <QRCodeSVG
                    value={pairing.qrPayload}
                    size={128}
                    bgColor="transparent"
                    fgColor="currentColor"
                    className="text-txt"
                  />
                ) : (
                  <Icon name="qr" size={52} />
                )}
              </div>
              <Lbl className="mt-2.5 text-txt-4">
                {pairing ? `CODE ${pairing.code}` : "NO ACTIVE QR"}
              </Lbl>
            </div>
            <div>
              <div className="font-display mb-1.5 text-[15px] font-semibold">Pair an Android phone</div>
              <p className="mb-3.5 text-[12px] leading-[1.6] text-txt-3">
                The phone scans this QR and connects over your local network. The visible QR is
                the local authorization, so the encrypted vault transfer starts automatically.
              </p>
              {pairingError && (
                <div className="mb-3 border border-danger bg-bg px-3 py-2 text-[11px] text-danger">
                  {pairingError}
                </div>
              )}
              <div className="mb-3 flex flex-wrap items-center gap-2">
                {!pairing ? (
                  <Btn onClick={startPairing} disabled={pairingBusy}>
                    <Icon name="qr" size={13} /> {pairingBusy ? "Starting..." : "Pair Android phone"}
                  </Btn>
                ) : (
                  <>
                    <Btn
                      onClick={approvePairing}
                      disabled={pairingBusy || !pairing.pendingDeviceName || pairing.approved}
                    >
                      <Icon name="check" size={13} />
                      {pairing.pendingDeviceName
                        ? `Auto approving ${pairing.pendingDeviceName}`
                        : "Waiting for phone"}
                    </Btn>
                    <Btn variant="ghost" onClick={cancelPairing} disabled={pairingBusy}>
                      <Icon name="x" size={13} /> Cancel
                    </Btn>
                  </>
                )}
                {pairing && (
                  <Chip tone={pairing.approved ? "accent" : pairing.pendingDeviceName ? "warn" : "default"}>
                    {pairing.approved
                      ? "APPROVED"
                      : pairing.pendingDeviceName
                        ? "PHONE WAITING"
                        : "SCAN QR"}
                  </Chip>
                )}
              </div>
              <div className="flex flex-col gap-px border border-line bg-line">
                {phases.map(([n, t, s]) => (
                  <div key={n} className="flex items-center gap-[11px] bg-bg-1 px-3 py-[9px]">
                    <span className="tabular-nums text-[10px] text-txt-4">{n}</span>
                    <span className="flex-1 text-[12px] text-txt-2">{t}</span>
                    <LblText className={s === "READY" ? "text-ok" : "text-txt-4"}>{s}</LblText>
                  </div>
                ))}
              </div>
              <div className="mt-4 border border-line bg-line">
                {(devices.length ? devices : []).map((device) => (
                  <div key={device.id} className="flex flex-wrap items-center gap-3 bg-bg-1 px-3 py-[10px]">
                    <Icon name="shield" size={13} />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[12px] text-txt">{device.name}</div>
                      <Lbl className={device.trusted ? "text-ok" : "text-txt-4"}>
                        {device.trusted ? "TRUSTED" : "REVOKED"} · {device.platform}
                      </Lbl>
                    </div>
                    {device.trusted ? (
                      <Btn variant="ghost" onClick={() => void revoke(device.id)}>
                        <Icon name="x" size={12} /> Revoke
                      </Btn>
                    ) : (
                      <Chip tone="default">REVOKED</Chip>
                    )}
                  </div>
                ))}
                {!devices.length && (
                  <div className="bg-bg-1 px-3 py-[10px] text-[11px] text-txt-4">
                    No paired devices yet.
                  </div>
                )}
              </div>
            </div>
          </div>
        </section>

        {/* AGENT IMPORT */}
        <section>
          <SectionHead icon="terminal" title="Agent Import" right="LOCAL JSON" />
          <SettingRow
            label="Import agent bundle"
            desc="Ask an agent to generate a MONOLITH JSON bundle, then paste, select, or drop it here while the vault is unlocked. Values are encrypted immediately; this screen only shows counts and redacted errors."
          >
            <div className="flex flex-col gap-2">
              <div className="border border-line-2 bg-bg-1 px-3 py-3 sm:w-[680px]">
                <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <LblText className="text-acc">LOCAL AGENT BRIDGE</LblText>
                    <div className="mt-1 text-[11px] leading-[1.5] text-txt-4">
                      Temporary loopback API. Agents can read capabilities and import bundles; they cannot read vault secrets.
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    <Chip tone={bridge ? "accent" : "default"}>{bridge ? "ACTIVE" : "OFF"}</Chip>
                    {bridge ? (
                      <Btn variant="ghost" onClick={() => void stopBridge()} disabled={bridgeBusy}>
                        <Icon name="x" size={13} /> Stop
                      </Btn>
                    ) : (
                      <Btn onClick={() => void startBridge()} disabled={bridgeBusy}>
                        <Icon name="terminal" size={13} /> {bridgeBusy ? "Starting..." : "Start bridge"}
                      </Btn>
                    )}
                  </div>
                </div>
                <div className="grid gap-px bg-line sm:grid-cols-2">
                  <KV k="Capabilities" v={bridge ? bridge.capabilitiesUrl : "start bridge first"} />
                  <KV k="Import endpoint" v={bridge ? bridge.importUrl : "start bridge first"} />
                </div>
                <div className="mt-2 text-[10.5px] leading-[1.5] text-txt-4">
                  Header: <span className="text-txt-2">X-MONOLITH-Agent-Token</span>. The copied prompt includes the temporary token.
                </div>
                {bridgeError && <div className="mt-2 text-[11px] text-danger">{bridgeError}</div>}
              </div>
              <div className="border border-line-2 bg-bg-1">
                <div className="flex flex-wrap items-center justify-between gap-2 border-b border-line px-3 py-2">
                  <LblText className="text-acc">AI AGENT HANDOFF</LblText>
                  <Btn variant="ghost" onClick={() => void copyAgentPrompt()}>
                    <Icon name={importPromptCopied ? "check" : "copy"} size={13} />
                    {importPromptCopied ? "Copied" : bridge ? "Copy API prompt" : "Copy prompt"}
                  </Btn>
                </div>
                <textarea
                  readOnly
                  value={agentPrompt}
                  spellCheck={false}
                  className="h-[250px] w-full resize-none border-0 bg-bg px-3 py-2 font-mono text-[10.5px] leading-[1.55] text-txt-3 outline-none sm:w-[680px]"
                />
              </div>
              <input
                ref={importFileRef}
                type="file"
                accept=".json,.monolith-import,.monolith-import.json,application/json"
                onChange={(event) => void onImportFileChange(event)}
                className="hidden"
              />
              <div
                onDragEnter={(event) => {
                  event.preventDefault();
                  setImportDropActive(true);
                }}
                onDragOver={(event) => {
                  event.preventDefault();
                  setImportDropActive(true);
                }}
                onDragLeave={() => setImportDropActive(false)}
                onDrop={(event) => void onImportDrop(event)}
                className={cn(
                  "flex flex-col gap-2 border border-dashed px-3 py-3 transition-colors sm:w-[680px]",
                  importDropActive ? "border-acc bg-acc/10" : "border-line-2 bg-bg-1",
                )}
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <LblText className="text-txt-2">DROP OR SELECT BUNDLE</LblText>
                    <div className="mt-1 text-[11px] leading-[1.5] text-txt-4">
                      Imports immediately through the same Rust path as manual edits.
                    </div>
                  </div>
                  <Btn variant="ghost" onClick={() => importFileRef.current?.click()} disabled={importBusy}>
                    <Icon name="upload" size={13} /> Select file
                  </Btn>
                </div>
              </div>
              <textarea
                value={importText}
                onChange={(event) => setImportText(event.currentTarget.value)}
                spellCheck={false}
                placeholder='{"version":1,"defaultProjectName":"Personal","items":[...]}'
                className="h-[150px] w-full min-w-0 resize-y border border-line-2 bg-bg-2 px-3 py-2 font-mono text-[11px] leading-[1.5] text-txt outline-none placeholder:text-txt-4 focus:border-acc sm:w-[680px]"
              />
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                  <Chip tone={importResult && !importResult.errors.length ? "accent" : "default"}>
                    {importResult
                      ? `${importResult.created} NEW · ${importResult.updated} UPDATED · ${importResult.skipped} SKIPPED`
                      : "READY"}
                  </Chip>
                  {importResult?.errors.length ? <Chip tone="danger">{importResult.errors.length} ERRORS</Chip> : null}
                </div>
                <Btn onClick={() => void runAgentImport()} disabled={importBusy || !importText.trim()}>
                  <Icon name="terminal" size={13} /> {importBusy ? "Importing..." : "Import JSON"}
                </Btn>
              </div>
              {importError && <div className="text-[11px] text-danger">{importError}</div>}
              {importResult?.errors.slice(0, 3).map((error) => (
                <div key={`${error.index}-${error.label}`} className="text-[11px] text-danger">
                  #{error.index + 1} {error.label}: {error.message}
                </div>
              ))}
            </div>
          </SettingRow>
        </section>

        {/* UPDATES */}
        <section>
          <SectionHead icon="refresh" title="Updates" right="SIGNED · GITHUB" />
          <SettingRow
            label="Check for desktop update"
            desc="Downloads only signed MONOLITH releases from GitHub. Updates replace app files and preserve your vault, settings, and paired devices in OS app-data."
          >
            <div className="flex flex-col gap-2 sm:items-end">
              <div className="flex flex-wrap items-center gap-2.5">
                <Chip tone={updateProgress?.phase === "none" ? "accent" : "default"}>
                  {updateProgressText(updateProgress)}
                </Chip>
                <Btn onClick={() => void runUpdater()} disabled={isMobile || updateBusy}>
                  <Icon name="refresh" size={13} /> {updateBusy ? "Updating..." : "Check & install"}
                </Btn>
              </div>
              {updateError && <div className="text-[11px] text-danger">{updateError}</div>}
              {isMobile && <div className="text-[11px] text-txt-4">Desktop updater is not used on Android builds.</div>}
            </div>
          </SettingRow>
          <SettingRow
            label="Data retention"
            desc="Updater and installer flows do not delete app-data. Data wipe remains an explicit in-app action only."
          >
            <Chip tone="accent">PRESERVE DATA</Chip>
          </SettingRow>
        </section>

        {/* ABOUT */}
        <section>
          <SectionHead icon="gear" title="About" />
          <div className="mb-[18px] grid grid-cols-1 gap-px bg-line min-[430px]:grid-cols-2 sm:grid-cols-[repeat(auto-fill,minmax(160px,1fr))]">
            <KV k="Version" v={`${__APP_VERSION__} · release`} />
            <KV k="Engine" v="Tauri 2 · Rust 2021" />
            <KV k="UI" v="React · TypeScript" />
            <KV k="Store" v="rusqlite · SQLite" />
          </div>
          <div className="border border-danger">
            <Lbl className="border-b border-danger px-[14px] py-2.5 text-danger">DANGER ZONE</Lbl>
            <div className="px-[14px] py-1">
              <SettingRow label="Lock vault now" desc="Zeroize the in-memory vault key and require the master password again.">
                <Btn variant="ghost" onClick={onLock}>
                  <Icon name="key" size={13} /> Lock
                </Btn>
              </SettingRow>
              <SettingRow label="Erase all data" desc="Would permanently wipe this vault. Not implemented yet." danger>
                <div className="flex items-center gap-2.5">
                  <Chip tone="default">PLANNED</Chip>
                  <Btn variant="danger" disabled>
                    <Icon name="trash" size={13} /> Erase
                  </Btn>
                </div>
              </SettingRow>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
