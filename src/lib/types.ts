/**
 * TypeScript DTOs mirroring the Rust serde structs in `src-tauri/src/models.rs`.
 * Keys are camelCase to match `#[serde(rename_all = "camelCase")]` on the Rust side.
 */

type FieldType = "text" | "password" | "api_key" | "url" | "email" | "json";

export type Environment = "production" | "staging" | "dev" | "all";

export interface ProjectIcon {
  /** "mono" | "glyph" | "img" */
  kind: string;
  name?: string;
  src?: string;
  color?: string;
}

interface ServiceMark {
  mono: string;
  color: string;
  slug?: string;
  icon?: string;
}

export interface Attachment {
  id: string;
  name: string;
  size: string;
  date: string;
}

export interface Project {
  id: string;
  name: string;
  sub: string;
  mono: string;
  color: string;
  icon?: ProjectIcon;
  created: string;
  updated: string;
  sortIndex: number;
  personal: boolean;
  count: number;
  totpCount: number;
  marks: ServiceMark[];
  files: Attachment[];
}

export interface FieldView {
  id: string;
  label: string;
  fieldType: FieldType;
  secret: boolean;
  danger: boolean;
  area: boolean;
  hasValue: boolean;
  /** Present only for non-secret fields. */
  value?: string;
}

export interface Service {
  id: string;
  projectId: string;
  templateId: string;
  templateName: string;
  mono: string;
  color: string;
  slug?: string;
  icon?: string;
  group: string;
  label: string;
  env: Environment;
  expiresAt?: string;
  title: string;
  updated: string;
  sortIndex: number;
  fields: FieldView[];
  totp: boolean;
  danger: boolean;
  strength?: number;
  fav: boolean;
  reused: boolean;
  exposed: boolean;
}

export interface Item {
  id: string;
  projectId: string;
  projectName: string;
  projectColor: string;
  projectMono: string;
  templateId: string;
  templateName: string;
  mono: string;
  color: string;
  slug?: string;
  icon?: string;
  label: string;
  env: Environment;
  expiresAt?: string;
  title: string;
  fieldCount: number;
  totp: boolean;
  danger: boolean;
  updated: string;
  created: string;
  fav: boolean;
  strength?: number;
  reused: boolean;
  exposed: boolean;
  tags: string[];
}

export interface RevealedSecret {
  fieldId: string;
  value: string;
}

export interface PasswordHistoryEntry {
  id: string;
  fieldId: string;
  serviceId: string;
  label: string;
  created: string;
}

export interface TotpCode {
  serviceId: string;
  code: string;
  remaining: number;
  period: number;
}

export interface Activity {
  time: string;
  action: string;
  target: string;
  kind: string;
}

export interface VaultStatus {
  initialized: boolean;
  unlocked: boolean;
  itemCount: number;
  vaultId?: string;
}

export interface Storage {
  used: string;
  total: string;
  pct: number;
}

export interface AppSettings {
  /** Undefined means "do not expire the remembered local unlock session automatically." */
  autoLockMs?: number;
  revealSecretsByDefault: boolean;
  clipboardClearMs: number;
}

export interface UpdateAppSettingsInput {
  autoLockMs?: number | null;
  revealSecretsByDefault: boolean;
  clipboardClearMs: number;
}

export interface PairedDevice {
  id: string;
  name: string;
  platform: string;
  trusted: boolean;
  revokedAt?: string;
  createdAt: string;
  lastSeenAt?: string;
}

export interface PairingSession {
  id: string;
  qrPayload: string;
  code: string;
  host: string;
  port: number;
  expiresAt: string;
  approved: boolean;
  pendingDeviceName?: string;
}

export interface PairingSessionStatus {
  id: string;
  approved: boolean;
  expired: boolean;
  pendingDeviceName?: string;
}

export interface PairingImportResult {
  vaultId: string;
  deviceId: string;
  itemCount: number;
}

export interface CompletePairingInput {
  qrPayload: string;
  deviceName?: string;
}

export type AppPlatform = "desktop" | "android" | "ios";

// --- template catalog ---

interface TemplateField {
  label: string;
  secret: boolean;
  danger: boolean;
  area: boolean;
  fieldType: FieldType;
}

export interface Template {
  id: string;
  name: string;
  mono: string;
  slug?: string;
  icon?: string;
  color: string;
  totp: boolean;
  group: string;
  fields: TemplateField[];
}

// --- command inputs ---

export interface CreateProjectInput {
  name: string;
  sub: string;
  color: string;
}

export interface UpdateProjectInput extends CreateProjectInput {
  projectId: string;
}

export interface ServiceFieldInput {
  label: string;
  value: string;
}

export interface AddServiceInput {
  projectId: string;
  templateId: string;
  label?: string;
  env?: Environment;
  expiresAt?: string;
  fields?: ServiceFieldInput[];
  totpSecret?: string;
}

export interface UpdateServiceInput {
  serviceId: string;
  label?: string;
  env?: Environment;
  expiresAt?: string;
  fields?: ServiceFieldInput[];
  totpSecret?: string;
}

export interface AgentImportBundle {
  version?: 1;
  source?: string;
  defaultProjectId?: string;
  defaultProjectName?: string;
  items: AgentImportItem[];
}

export interface AgentImportItem {
  projectId?: string;
  projectName?: string;
  templateId: string;
  label?: string;
  env?: Environment;
  expiresAt?: string;
  fields?: ServiceFieldInput[];
  totpSecret?: string;
  source?: string;
}

export interface AgentImportError {
  index: number;
  label: string;
  message: string;
}

export interface AgentImportResult {
  created: number;
  updated: number;
  skipped: number;
  errors: AgentImportError[];
}

export interface AgentBridgeSession {
  baseUrl: string;
  capabilitiesUrl: string;
  projectsUrl: string;
  importUrl: string;
  token: string;
  expiresAt: string;
}

// --- errors ---

type AppErrorKind =
  | "locked"
  | "badPassword"
  | "vaultState"
  | "notFound"
  | "invalid"
  | "crypto"
  | "db"
  | "other";

export interface AppError {
  kind: AppErrorKind;
  message: string;
}
