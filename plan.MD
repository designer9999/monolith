Here’s the direction I’d build: **a local-first Tauri 2 desktop app with a Rust vault core, React/TypeScript UI, encrypted SQLite storage, and mobile support planned from day one.** Tauri 2 is a good fit because it lets you build desktop and mobile targets from one codebase, with Rust for sensitive logic and a web UI for the polished visual dashboard you want. Tauri’s docs also show it supports officially maintained frontend templates, including React, Svelte, Vue, Solid, Angular, Yew, Leptos, and others. ([Tauri][1])

## 1. Recommended tech stack

### Core choice

Use **Tauri 2 + Rust + React + TypeScript + Tailwind + shadcn/ui**.

I would not build this as a normal web app first, because browser storage, extensions, XSS, cached state, and accidental cloud deployment are all bad fits for a personal secrets manager. Tauri still uses a WebView, so it is not magically secure, but it gives you a much better local app boundary: the UI can be pretty, while the Rust backend owns encryption, database access, TOTP generation, clipboard handling, imports, exports, and future phone sync. Tauri’s command system is specifically designed for calling Rust functions from the frontend, and its capabilities system lets you constrain which permissions a window or WebView receives. ([Tauri][2])

| Area                   | Recommendation                                                | Why                                                                                                                                            |
| ---------------------- | ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| App shell              | **Tauri 2**                                                   | Desktop now; mobile later with `tauri android dev` / `tauri ios dev`. ([Tauri][3])                                                             |
| Backend                | **Rust 2024 edition**                                         | Strong type safety and good crypto libraries. Rust 2024 is stable from Rust 1.85.0 onward. ([Rust Blog][4])                                    |
| Frontend               | **React + TypeScript + Vite**                                 | Best ecosystem for drag/drop, shadcn/ui, dashboard layouts, and future mobile UI reuse.                                                        |
| UI styling             | **Tailwind CSS + shadcn/ui + Lucide icons**                   | Tailwind is utility-first, shadcn/ui gives customizable component code, and Lucide provides optimized SVG icon components. ([Tailwind CSS][5]) |
| Drag/drop              | **dnd-kit**                                                   | Current dnd-kit React docs provide draggable, droppable, sortable hooks, and drag/drop event handling. ([dnd kit][6])                          |
| Storage                | **SQLite + field-level encryption**, optionally **SQLCipher** | SQLite is simple and local. SQLCipher adds full-database encryption with 256-bit AES and tamper-resistant design. ([zetetic.net][7])           |
| Encryption             | **Argon2id + XChaCha20Poly1305**                              | Argon2id for master-password key derivation; XChaCha20Poly1305 for authenticated encryption. ([Docs.rs][8])                                    |
| TOTP                   | **totp-rs**                                                   | Rust TOTP library with configurable algorithm, digits, validity period, and skew. TOTP itself is standardized by RFC 6238. ([Docs.rs][9])      |
| Secret memory handling | **secrecy + zeroize**                                         | Helps avoid accidental debug leaks and wipes secret values on drop where possible. ([Docs.rs][10])                                             |
| Future phone unlock    | **Tauri biometric plugin**                                    | Can check biometric availability and prompt device biometric authentication on supported platforms. ([Tauri][11])                              |
| QR import later        | **Tauri barcode-scanner plugin**                              | Mobile QR scanning support for TOTP import and device pairing. ([Tauri][12])                                                                   |

## 2. Project setup

Start with Tauri 2, React, and TypeScript:

```bash
pnpm create tauri-app@latest secret-vault
cd secret-vault
```

Choose:

```text
Frontend: React
Language: TypeScript
Package manager: pnpm
```

Add UI and app dependencies:

```bash
pnpm add @dnd-kit/react @dnd-kit/helpers lucide-react
pnpm dlx shadcn@latest init
pnpm tauri add dialog clipboard-manager opener updater
```

Add Rust dependencies from inside `src-tauri`:

```bash
cd src-tauri

cargo add serde serde_json thiserror uuid time
cargo add argon2 chacha20poly1305 rand_core secrecy zeroize
cargo add totp-rs
cargo add rusqlite --features bundled-sqlcipher-vendored-openssl
```

The `rusqlite` SQLCipher feature `bundled-sqlcipher-vendored-openssl` is available for building with bundled SQLCipher and vendored OpenSSL; that is useful when you want fewer system dependency surprises across machines. ([Docs.rs][13])

## 3. High-level app architecture

Think of the app as **three layers**:

```text
React UI
  ↓ Tauri invoke commands
Rust command/API layer
  ↓
Vault core: encryption, DB, templates, TOTP, file import/export, future sync
```

The React side should never directly manage raw secrets more than necessary. The Rust side should expose carefully designed commands:

```rust
#[tauri::command]
async fn unlock_vault(master_password: String) -> Result<UnlockResult, AppError>;

#[tauri::command]
async fn lock_vault() -> Result<(), AppError>;

#[tauri::command]
async fn list_projects() -> Result<Vec<ProjectCard>, AppError>;

#[tauri::command]
async fn create_project(input: CreateProjectInput) -> Result<ProjectCard, AppError>;

#[tauri::command]
async fn add_service_from_template(
    project_id: String,
    template_id: String,
    label: Option<String>,
) -> Result<ServiceView, AppError>;

#[tauri::command]
async fn set_secret_field(
    service_id: String,
    field_key: String,
    value: String,
) -> Result<(), AppError>;

#[tauri::command]
async fn reveal_secret_field(
    field_id: String,
) -> Result<RevealedSecret, AppError>;

#[tauri::command]
async fn copy_secret_field(
    field_id: String,
    clear_after_seconds: u64,
) -> Result<(), AppError>;

#[tauri::command]
async fn generate_totp_code(
    totp_id: String,
) -> Result<TotpCodeView, AppError>;
```

When `master_password: String` arrives from the frontend, immediately wrap it in `SecretString`, derive the key, and clear temporary buffers. Rust cannot make process memory perfectly safe against malware or memory dumping, but `secrecy` and `zeroize` help prevent accidental leaks through logs, debug output, and normal drop paths. ([Docs.rs][10])

## 4. Vault security design

### First-run vault creation

Use an **envelope encryption** design:

```text
master password
  ↓ Argon2id
wrapping key
  ↓ decrypts/encrypts
random vault key
  ↓ encrypts/decrypts
individual project/service/field secrets
```

On first run:

1. Generate a random 32-byte `vault_key`.
2. Ask for a master password.
3. Generate a random salt.
4. Derive a 32-byte wrapping key using **Argon2id**.
5. Encrypt `vault_key` using `XChaCha20Poly1305`.
6. Store only the salt, KDF parameters, encrypted vault key, and nonce in the database header.

On unlock:

1. User enters master password.
2. Rust derives the wrapping key.
3. Rust decrypts the `vault_key`.
4. Store the unlocked `vault_key` only in Rust memory.
5. Auto-lock after inactivity.
6. On lock, zeroize the in-memory key.

OWASP recommends strong, slow password hashing such as Argon2id, and its current cheat sheet lists Argon2id with at least 19 MiB memory, 2 iterations, and parallelism 1 as a minimum configuration. For a local desktop app, you can tune higher, such as 64–128 MiB, as long as unlock remains comfortable on your machines. ([OWASP Cheat Sheet Series][14])

### Secret field encryption

Encrypt every secret value separately:

```text
field plaintext
  + random 24-byte nonce
  + associated data: vault_id | project_id | service_id | field_id | schema_version
  ↓ XChaCha20Poly1305
ciphertext + auth tag
```

Use **associated data** so a ciphertext cannot be silently moved from one project/service/field to another. The `chacha20poly1305` crate supports XChaCha20Poly1305 with an extended 192-bit, 24-byte nonce. ([Docs.rs][15])

### SQLCipher or not?

I would do both, but in stages:

**Stage 1:** SQLite metadata + field-level encryption.
**Stage 2:** Add SQLCipher to encrypt the entire database file too.

Field-level encryption protects the most important values even if the database layer is misconfigured. SQLCipher adds another layer by encrypting the whole SQLite database, including metadata, with 256-bit AES. ([zetetic.net][7])

## 5. Data model

Use UUIDs everywhere and keep a `sort_index` for drag/drop ordering.

### Main tables

```sql
CREATE TABLE vault_meta (
  id TEXT PRIMARY KEY,
  schema_version INTEGER NOT NULL,
  kdf_algorithm TEXT NOT NULL,
  kdf_params_json TEXT NOT NULL,
  kdf_salt BLOB NOT NULL,
  vault_key_nonce BLOB NOT NULL,
  encrypted_vault_key BLOB NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  slug TEXT,
  description TEXT,
  logo_asset_id TEXT,
  accent_color TEXT,
  sort_index INTEGER NOT NULL,
  archived INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE service_instances (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  template_id TEXT NOT NULL,
  label TEXT NOT NULL,
  icon TEXT,
  environment TEXT, -- dev, staging, production, all
  sort_index INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE secret_fields (
  id TEXT PRIMARY KEY,
  service_id TEXT NOT NULL,
  field_key TEXT NOT NULL,
  label TEXT NOT NULL,
  field_type TEXT NOT NULL, -- password, api_key, url, email, json, textarea, file_ref
  is_secret INTEGER NOT NULL DEFAULT 1,
  encrypted_value BLOB,
  nonce BLOB,
  metadata_json TEXT,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(service_id) REFERENCES service_instances(id) ON DELETE CASCADE,
  UNIQUE(service_id, field_key)
);

CREATE TABLE totp_accounts (
  id TEXT PRIMARY KEY,
  service_id TEXT NOT NULL,
  issuer TEXT,
  account_label TEXT,
  algorithm TEXT NOT NULL DEFAULT 'SHA1',
  digits INTEGER NOT NULL DEFAULT 6,
  period INTEGER NOT NULL DEFAULT 30,
  encrypted_secret BLOB NOT NULL,
  nonce BLOB NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY(service_id) REFERENCES service_instances(id) ON DELETE CASCADE
);

CREATE TABLE assets (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL, -- project_logo, attachment, icon
  mime_type TEXT NOT NULL,
  file_name TEXT,
  encrypted_blob_path TEXT,
  nonce BLOB,
  sha256 TEXT,
  created_at TEXT NOT NULL
);
```

For a beautiful UI, project names and service labels can stay plaintext so the dashboard loads quickly. If even project names are sensitive, encrypt them too, but you’ll lose easy search and sorting unless you add a more complex encrypted-search design.

## 6. Service template system

Templates should be versioned JSON files bundled with the app. You can also add “custom templates” later and store those in the database.

Example:

```json
{
  "id": "supabase",
  "version": 1,
  "displayName": "Supabase",
  "icon": "supabase",
  "defaultEnvironment": "production",
  "fields": [
    {
      "key": "project_url",
      "label": "Project URL",
      "type": "url",
      "secret": false,
      "copyable": true
    },
    {
      "key": "anon_key",
      "label": "Anon Public API Key",
      "type": "api_key",
      "secret": true,
      "copyable": true
    },
    {
      "key": "service_role_key",
      "label": "Service Role Key",
      "type": "api_key",
      "secret": true,
      "copyable": true,
      "danger": true
    },
    {
      "key": "database_password",
      "label": "Database Password",
      "type": "password",
      "secret": true,
      "copyable": true
    },
    {
      "key": "s3_access_key_id",
      "label": "S3 Access Key ID",
      "type": "text",
      "secret": true,
      "copyable": true
    },
    {
      "key": "s3_secret_access_key",
      "label": "S3 Secret Access Key",
      "type": "password",
      "secret": true,
      "copyable": true
    }
  ],
  "totp": {
    "supported": true
  }
}
```

Start with these built-in templates:

| Template   | Fields                                                                                                                                  |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| Supabase   | Project URL, anon key, service role key, DB host, DB name, DB user, DB password, JWT secret, S3 access key, S3 secret, storage endpoint |
| Google     | Client ID, client secret, redirect URIs, project ID, service account JSON, OAuth refresh token, email, password, TOTP                   |
| GitHub     | Username, password, personal access token, SSH key, deploy key, repo URL, webhook secret, TOTP                                          |
| Vercel     | Account email, team ID, project ID, API token, deploy hook URL, environment variables                                                   |
| Stripe     | Publishable key, secret key, restricted key, webhook secret, account email, test/live mode                                              |
| Cloudflare | Account ID, zone ID, API token, Global API key warning, Turnstile site key, Turnstile secret                                            |
| Database   | Host, port, database, username, password, connection string, SSL mode                                                                   |
| Email/SMTP | Host, port, username, password, from email, provider notes                                                                              |
| Domain/DNS | Registrar login, DNS provider login, nameservers, renewal date, TOTP                                                                    |
| Custom     | User-defined fields                                                                                                                     |

The template system should create empty fields automatically. The user should not have to remember what Supabase, Stripe, GitHub, Vercel, or Google usually needs.

## 7. TOTP / authenticator support

TOTP secrets are real secrets. Store them encrypted like passwords. Do not store the raw `otpauth://` URI unless it is encrypted too.

Flow:

```text
User adds TOTP secret or scans/imports otpauth URI
  ↓
Rust parses issuer/account/secret/algorithm/digits/period
  ↓
Rust encrypts TOTP shared secret
  ↓
UI shows rotating 6-digit code
  ↓
Rust generates code on demand and returns only the code + seconds remaining
```

TOTP is the time-based version of HOTP, replacing the HOTP counter with a value derived from time and a time step, as specified in RFC 6238. ([IETF Datatracker][16]) The common `otpauth://TYPE/LABEL?PARAMETERS` URI format includes parameters such as `secret`, `issuer`, `algorithm`, `digits`, and `period`; Google’s authenticator wiki documents the default 6 digits and 30-second period convention. ([GitHub][17])

Important security note: storing passwords and TOTP seeds in the same vault is convenient, but it weakens the “two separate factors” idea. For your own personal workflow, this may be acceptable, but for highly sensitive accounts, keeping TOTP in a separate authenticator or hardware key is safer.

## 8. Tauri security settings

Use **strict capabilities**. Do not expose broad filesystem or shell access to the frontend. Tauri’s capabilities system is there to define what permissions are granted or denied for each window/WebView. ([Tauri][18])

A starting `src-tauri/capabilities/main.json` could look like this:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "main",
  "description": "Main vault window permissions",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:allow-open",
    "dialog:allow-save",
    "clipboard-manager:allow-write-text",
    "clipboard-manager:allow-clear"
  ]
}
```

The clipboard plugin intentionally enables no permissions by default because clipboard access is inherently dangerous; write and clear permissions must be explicitly enabled. ([Tauri][19]) For this app, **avoid `clipboard-manager:allow-read-text`** unless you really need it. Copying secrets out is useful; reading the user’s clipboard is not necessary.

For filesystem access, prefer Rust backend code that reads/writes only inside the app data directory. If you use the Tauri filesystem plugin from JavaScript, scope it tightly. Tauri’s filesystem API prevents path traversal and requires path scopes using glob patterns. ([Tauri][20])

Example scoped filesystem permission only for app data:

```json
{
  "identifier": "fs:scope",
  "allow": [{ "path": "$APPDATA/secret-vault/**" }]
}
```

For `tauri.conf.json`, use a strict production CSP. Tauri’s CSP feature reduces the impact of common WebView vulnerabilities such as XSS, and Tauri can hash local scripts and nonce allowed styles/scripts. ([Tauri][21])

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Secret Vault",
  "version": "0.1.0",
  "identifier": "com.yourname.secretvault",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist",
    "removeUnusedCommands": true
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "Secret Vault",
        "width": 1280,
        "height": 820,
        "minWidth": 980,
        "minHeight": 680,
        "resizable": true,
        "center": true,
        "devtools": false
      }
    ],
    "security": {
      "csp": "default-src 'self'; connect-src ipc: http://ipc.localhost; script-src 'self'; style-src 'self'; img-src 'self' data: asset:; object-src 'none'; base-uri 'none'; frame-ancestors 'none'"
    }
  }
}
```

Tauri’s config reference shows `frontendDist`, `devUrl`, `removeUnusedCommands`, CSP configuration, and `devtools` as configurable fields. It also warns that disabling Tauri’s CSP modification can make the app vulnerable to XSS, so keep `dangerousDisableAssetCspModification` off unless you deeply understand the consequences. ([Tauri][22])

## 9. UI structure

Build it like a visual vault, not like a database admin screen.

### Landing page

Show project folders/cards:

```text
[ Logo ] Client Project A
         8 services · 2FA · Production

[ Logo ] SaaS App
         Supabase · Vercel · Stripe · GitHub

[ + New Project ]
```

Features:

- Search projects.
- Sort by drag/drop.
- Filter by tag: client, personal, production, archived.
- Add custom logo, icon, accent color.
- “Recently used” row.
- Locked state overlay when vault is locked.

### Project detail page

Use a split layout:

```text
Left sidebar:
  Supabase
  GitHub
  Vercel
  Stripe
  Google
  Custom Notes

Main panel:
  Supabase
    Project URL        [copy]
    Anon Key           [••••••••] [reveal] [copy]
    Service Role Key   [danger] [••••••••] [reveal] [copy]
    TOTP               123 456   18s remaining
```

### Secret field component

Every secret field should have:

```text
Label
Masked value
Reveal button
Copy button
Last updated
Danger badge for powerful keys
Optional notes
```

Never reveal all secrets by default. Reveal one value at a time.

### TOTP display

Show:

```text
123 456
valid for 18s
[copy code]
```

The frontend timer can animate the countdown, but Rust should generate the actual code.

## 10. Future phone connection plan

Because Tauri 2 supports mobile targets, design the vault file and Rust core so the same logic can run on desktop and phone. Tauri’s mobile dev flow uses `tauri android dev` or `tauri ios dev`, and physical iOS device development has special host configuration requirements. ([Tauri][3])

Recommended sync roadmap:

**Phase 1: Desktop only**

- Local encrypted vault.
- Encrypted backup/export file.
- No cloud.

**Phase 2: Mobile companion**

- Same vault schema.
- Local encrypted vault on phone.
- Unlock with master password.
- Optional biometric convenience unlock using Tauri biometric plugin. ([Tauri][11])

**Phase 3: Device pairing**

- Desktop shows QR code.
- Phone scans QR code using barcode scanner plugin.
- Establish an encrypted pairing session.
- Transfer encrypted vault over local network or via an encrypted export file. ([Tauri][12])

**Phase 4: Optional encrypted cloud sync**

Only sync ciphertext. The cloud provider should never see raw passwords, API keys, TOTP seeds, project notes, or vault keys. OWASP’s secrets management guidance distinguishes server-side encryption from client-side encryption; for your app, client-side encryption is the right model because secrets stay encrypted until your own device decrypts them. ([OWASP Cheat Sheet Series][23])

## 11. Security pitfalls to avoid

Do not put secrets in:

```text
localStorage
sessionStorage
IndexedDB
frontend logs
console.log
crash reports
analytics tools
plain JSON exports
unencrypted SQLite fields
screenshots
clipboard longer than needed
```

Do not grant broad Tauri permissions:

```text
fs:read-all
fs:write-all
shell access
arbitrary opener access
remote HTTP access
clipboard read access
```

Do not store the master password in the OS keychain by default. The Rust `keyring` ecosystem can connect to platform credential stores, but automatic unlock changes your threat model: anyone with access to your logged-in OS account may effectively get vault access. ([Docs.rs][24]) Use keychain/biometric only as an explicit convenience mode after you already have a strong master-password flow.

Do not rely on encryption alone. OWASP’s cryptographic storage guidance says the first step is threat modeling, because architecture determines whether the crypto actually protects the data. ([OWASP Cheat Sheet Series][25]) Your realistic protection goal should be: protect secrets at rest, protect backups, reduce accidental leaks, and auto-lock when idle. This will not protect against malware, a keylogger, a malicious browser extension reading copied secrets after paste, screen recording, or someone using your unlocked machine.

For updates, either update manually or use Tauri’s updater with signing. Tauri’s updater requires signatures and uses a public/private signing key pair; the private signing key must never be shared or lost. ([Tauri][26])

## 12. Build milestones

### Milestone 1: Vault foundation

Build:

- First-run master password setup.
- Argon2id key derivation.
- Vault key wrapping.
- SQLite schema.
- Create/list projects.
- Lock/unlock.
- Auto-lock after inactivity.

No templates yet. No TOTP yet. Make the vault secure first.

### Milestone 2: Visual project dashboard

Build:

- Project folder cards.
- Logo/icon upload.
- Accent colors.
- Drag/drop ordering.
- Search.
- Archive/delete.

Use dnd-kit for project sorting and later for service sorting. ([dnd kit][6])

### Milestone 3: Service templates

Build:

- Template registry.
- Add Supabase/GitHub/Vercel/Stripe/Google services.
- Dynamic form rendering from template JSON.
- Store encrypted field values.
- Reveal/copy individual fields.

### Milestone 4: TOTP

Build:

- Add manual TOTP secret.
- Import `otpauth://` URI.
- Generate rotating code in Rust.
- Copy code.
- Optional QR import later on mobile.

### Milestone 5: Files and attachments

Build:

- Drag files into a project.
- Encrypt file attachments.
- Store encrypted blobs under app data.
- Show file cards with type, name, date, and notes.

### Milestone 6: Backup/export

Build:

- Encrypted vault export.
- Encrypted vault import.
- Recovery warning: no master password means no recovery.
- Optional printed emergency recovery phrase for a secondary wrapped vault key.

### Milestone 7: Mobile

Build:

- Tauri Android/iOS target.
- Same Rust vault core.
- Biometric convenience unlock.
- QR scan for TOTP and device pairing.
- Local-only encrypted sync.

## 13. Clean module layout

Use this project structure:

```text
src/
  app/
    App.tsx
    routes/
      Dashboard.tsx
      ProjectDetail.tsx
      Settings.tsx
      Unlock.tsx
    components/
      ProjectCard.tsx
      ServiceCard.tsx
      SecretField.tsx
      TotpCode.tsx
      TemplatePicker.tsx
      LogoPicker.tsx
    lib/
      tauriCommands.ts
      types.ts

src-tauri/
  src/
    lib.rs
    commands/
      mod.rs
      vault_commands.rs
      project_commands.rs
      service_commands.rs
      totp_commands.rs
      import_export_commands.rs
    vault/
      mod.rs
      crypto.rs
      key_derivation.rs
      unlock_state.rs
      file_format.rs
    db/
      mod.rs
      migrations.rs
      repositories.rs
    templates/
      mod.rs
      builtin.rs
      schema.rs
    totp/
      mod.rs
    sync/
      mod.rs
    error.rs
  capabilities/
    main.json
  migrations/
    001_init.sql
  templates/
    supabase.json
    github.json
    vercel.json
    stripe.json
    google.json
```

## 14. The safest first version

For your personal project, the first production-ready version should be:

```text
Tauri 2 desktop app
Local-only
No account system
No cloud
No analytics
No remote scripts
No plaintext exports
Rust-only encryption
SQLite + encrypted fields
Master password unlock
Auto-lock
Copy with clipboard auto-clear
TOTP generated in Rust
Encrypted backup file
```

That gives you the beautiful project-folder workflow you want while keeping the security model understandable. Once that is solid, add phone support as a second local encrypted vault using the same Rust core.

[1]: https://v2.tauri.app/?utm_source=chatgpt.com "Tauri 2.0 | Tauri"
[2]: https://v2.tauri.app/develop/calling-rust/ "Calling Rust from the Frontend | Tauri"
[3]: https://v2.tauri.app/develop/ "Develop | Tauri"
[4]: https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/?utm_source=chatgpt.com "Announcing Rust 1.85.0 and Rust 2024"
[5]: https://tailwindcss.com/?utm_source=chatgpt.com "Tailwind CSS - Rapidly build modern websites without ever ..."
[6]: https://dndkit.com/react/quickstart "React Quickstart - dnd kit"
[7]: https://www.zetetic.net/sqlcipher/ "SQLCipher - Full Database Encryption for SQLite | Zetetic"
[8]: https://docs.rs/argon2 "argon2 - Rust"
[9]: https://docs.rs/totp-rs "totp_rs - Rust"
[10]: https://docs.rs/zeroize/latest/zeroize/ "zeroize - Rust"
[11]: https://v2.tauri.app/plugin/biometric/ "Biometric | Tauri"
[12]: https://v2.tauri.app/plugin/barcode-scanner/ "Barcode Scanner | Tauri"
[13]: https://docs.rs/crate/rusqlite/latest?utm_source=chatgpt.com "rusqlite 0.40.0"
[14]: https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html "Password Storage - OWASP Cheat Sheet Series"
[15]: https://docs.rs/chacha20poly1305 "chacha20poly1305 - Rust"
[16]: https://datatracker.ietf.org/doc/html/rfc6238 "
            
                RFC 6238 - TOTP: Time-Based One-Time Password Algorithm
            
        "
[17]: https://github.com/google/google-authenticator/wiki/Key-Uri-Format "Key Uri Format · google/google-authenticator Wiki · GitHub"
[18]: https://v2.tauri.app/security/capabilities/ "Capabilities | Tauri"
[19]: https://v2.tauri.app/plugin/clipboard/ "Clipboard | Tauri"
[20]: https://v2.tauri.app/reference/javascript/fs/ "@tauri-apps/plugin-fs | Tauri"
[21]: https://v2.tauri.app/security/csp/ "Content Security Policy (CSP) | Tauri"
[22]: https://v2.tauri.app/reference/config/ "Configuration | Tauri"
[23]: https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html "Secrets Management - OWASP Cheat Sheet Series"
[24]: https://docs.rs/keyring "keyring - Rust"
[25]: https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html "Cryptographic Storage - OWASP Cheat Sheet Series"
[26]: https://v2.tauri.app/plugin/updater/ "Updater | Tauri"
