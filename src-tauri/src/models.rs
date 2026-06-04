//! Data-transfer objects shared with the frontend.
//!
//! All structs serialize with `camelCase` keys so the TypeScript layer in
//! `src/lib/types.ts` mirrors them 1:1. Secret *values* are never included here
//! by default — they are revealed one at a time through dedicated commands.

use serde::{Deserialize, Serialize};

/// A field type hint that drives input rendering and copy behaviour in the UI.
/// Mirrors the design's `ftype` values plus a default `text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    #[default]
    Text,
    Password,
    ApiKey,
    Url,
    Email,
    Json,
}

/// The environment a service instance belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Production,
    Staging,
    Dev,
    #[default]
    All,
}

/// A project's icon — monogram (default), a named glyph, or an uploaded image
/// (stored as a data URL on disk in the project record).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIcon {
    /// "mono" | "glyph" | "img"
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A small brand mark shown on a project card to preview its services.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceMark {
    pub mono: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// A project folder shown on the home grid and sidebar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub sub: String,
    pub mono: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<ProjectIcon>,
    pub created: String,
    pub updated: String,
    pub sort_index: i64,
    pub personal: bool,
    /// Number of service instances in the project.
    pub count: i64,
    /// Number of those services that have TOTP enabled.
    pub totp_count: i64,
    /// Distinct service marks (up to a handful) for the card preview.
    pub marks: Vec<ServiceMark>,
    pub files: Vec<Attachment>,
}

/// One non-secret field summary returned with a service. Secret values are
/// masked — only `hasValue` indicates whether a value is stored.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldView {
    pub id: String,
    pub label: String,
    pub field_type: FieldType,
    pub secret: bool,
    pub danger: bool,
    pub area: bool,
    pub has_value: bool,
    /// Present only for non-secret fields (the design shows these in clear text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// A configured service instance inside a project, with its fields.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub project_id: String,
    pub template_id: String,
    pub template_name: String,
    pub mono: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub group: String,
    pub label: String,
    pub env: Environment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub title: String,
    pub updated: String,
    pub sort_index: i64,
    pub fields: Vec<FieldView>,
    pub totp: bool,
    pub danger: bool,
    /// Min password strength across this service's password-like fields (0-100), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<u8>,
    pub fav: bool,
    pub reused: bool,
    pub exposed: bool,
}

/// A flattened item used by the "All Items" browser and home widgets — a service
/// projected with its parent-project context.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub project_id: String,
    pub project_name: String,
    pub project_color: String,
    pub project_mono: String,
    pub template_id: String,
    pub template_name: String,
    pub mono: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub label: String,
    pub env: Environment,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub title: String,
    pub field_count: i64,
    pub totp: bool,
    pub danger: bool,
    pub updated: String,
    pub created: String,
    pub fav: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strength: Option<u8>,
    pub reused: bool,
    pub exposed: bool,
    pub tags: Vec<String>,
}

/// A file attachment recorded against a project. Only metadata (name, size,
/// date) is stored today; encrypting the file bytes is planned.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub size: String,
    pub date: String,
}

/// A revealed secret value (returned only by the explicit reveal command).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealedSecret {
    pub field_id: String,
    pub value: String,
}

/// A project-scoped reusable field value shown only inside add/edit forms.
/// Secret suggestions are decrypted only for this explicit command while the
/// vault is unlocked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldSuggestion {
    pub field_id: String,
    pub service_id: String,
    pub service_title: String,
    pub template_name: String,
    pub field_label: String,
    pub field_type: FieldType,
    pub secret: bool,
    pub value: String,
    pub updated: String,
}

/// One archived previous secret value. The value itself is revealed through a
/// separate explicit command, just like the active secret field.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordHistoryEntry {
    pub id: String,
    pub field_id: String,
    pub service_id: String,
    pub label: String,
    pub created: String,
}

/// A generated TOTP code with the seconds remaining in the current step.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpCode {
    pub service_id: String,
    pub code: String,
    pub remaining: u32,
    pub period: u32,
}

/// A recent-activity entry shown on the home screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub time: String,
    pub action: String,
    pub target: String,
    pub kind: String,
}

/// The state returned after unlocking: whether a vault existed and the count.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub initialized: bool,
    pub unlocked: bool,
    pub item_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_id: Option<String>,
}

/// Storage usage summary for the sidebar footer and settings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Storage {
    pub used: String,
    pub total: String,
    pub pct: u8,
}

/// Vault-scoped application settings. These live in SQLite so encrypted
/// desktop-to-phone pairing copies them with the vault snapshot.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Null means "do not expire the remembered local unlock session automatically".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_lock_ms: Option<i64>,
    pub reveal_secrets_by_default: bool,
    pub clipboard_clear_ms: i64,
}

/// Partial settings update from the UI.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAppSettingsInput {
    pub auto_lock_ms: Option<i64>,
    pub reveal_secrets_by_default: bool,
    pub clipboard_clear_ms: i64,
}

/// A trusted paired device that can receive local vault transfers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub trusted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<String>,
}

/// Desktop-side one-time QR pairing session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingSession {
    pub id: String,
    pub qr_payload: String,
    pub code: String,
    pub host: String,
    pub port: u16,
    pub expires_at: String,
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_device_name: Option<String>,
}

/// Status returned while a desktop pairing session waits for a phone.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingSessionStatus {
    pub id: String,
    pub approved: bool,
    pub expired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_device_name: Option<String>,
}

/// Result returned by the mobile side after scanning and importing a vault.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingImportResult {
    pub vault_id: String,
    pub device_id: String,
    pub item_count: i64,
}

/// A temporary localhost API session for local AI agents. The token is shown
/// only so the user can hand it to a local agent; endpoints never reveal stored
/// secrets and can only import while the vault is unlocked.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBridgeSession {
    pub base_url: String,
    pub capabilities_url: String,
    pub projects_url: String,
    pub import_url: String,
    pub token: String,
    pub expires_at: String,
}

/// Input used by the mobile side after scanning a desktop QR payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletePairingInput {
    pub qr_payload: String,
    #[serde(default)]
    pub device_name: String,
}

// --- inputs (deserialized from the frontend) ---

/// Input for creating a project.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub sub: String,
    pub color: String,
}

/// Input for editing project metadata.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectInput {
    pub project_id: String,
    pub name: String,
    pub sub: String,
    pub color: String,
}

/// One filled field when adding a service from a template.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceFieldInput {
    pub label: String,
    pub value: String,
}

/// Input for adding a service to a project from a template.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddServiceInput {
    pub project_id: String,
    pub template_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub env: Environment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub fields: Vec<ServiceFieldInput>,
    /// Optional base32 TOTP secret to enable rotating codes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_secret: Option<String>,
}

/// Input for editing an existing service. Secret fields are only changed when a
/// non-empty value is supplied; blanks mean "leave this secret unchanged".
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateServiceInput {
    pub service_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub env: Environment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub fields: Vec<ServiceFieldInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_secret: Option<String>,
}

/// Machine-readable local import bundle. This is intentionally boring JSON so
/// another AI agent or script can generate it without knowing MONOLITH internals.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentImportBundle {
    #[serde(default)]
    pub version: Option<u8>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub default_project_id: Option<String>,
    #[serde(default)]
    pub default_project_name: Option<String>,
    #[serde(default)]
    pub items: Vec<AgentImportItem>,
}

/// One service to create or update from an agent import bundle.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentImportItem {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    pub template_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub env: Environment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub fields: Vec<ServiceFieldInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_secret: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

/// Per-item import problem. Secret values are never echoed in these messages.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentImportError {
    pub index: usize,
    pub label: String,
    pub message: String,
}

/// Summary returned after importing a bundle.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentImportResult {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<AgentImportError>,
}
