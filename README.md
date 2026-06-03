# MONOLITH — local-first credential & secrets vault

A brutalist-tech desktop password manager. Project-folder-centric: each folder is
a project, and inside it you store all that project's credentials (Supabase,
GitHub, Vercel, Stripe, …) from preset service templates, with live 2FA codes and
reveal/copy. (File attachments are recorded but not yet encrypted — see "Status".)

**Stack:** Rust 2021 + Tauri 2 · React 19 + TypeScript + Vite 8 · Tailwind CSS 4 +
shadcn/ui. Local-first — no account, no cloud, nothing leaves the device.

## Security model

Envelope encryption, all owned by the Rust core:

```
master password ──Argon2id (64 MiB · t=3 · p=1)──▶ wrapping key
wrapping key ──unwraps──▶ random 32-byte vault key
vault key ──XChaCha20-Poly1305 (per field, AAD-bound)──▶ every secret + TOTP seed
```

- The plaintext vault key lives only in memory (zeroized on lock).
- Every secret value and TOTP seed is encrypted at the field level with
  **XChaCha20-Poly1305**, bound to its `project|service|field` via associated data so
  a ciphertext can't be relocated. (No AES / no SQLCipher — see "Status" below.)
- Storage is plain bundled **SQLite** in the OS app-data directory. The secret
  *values* are encrypted; project/service names and labels are stored in clear text
  for a fast, searchable UI. Whole-database encryption (SQLCipher) is **planned**, not
  active.
- TOTP codes are real RFC 6238 (HMAC-SHA1), generated in Rust on demand.
- Least-privilege Tauri capabilities: window controls and clipboard write/clear
  only. No clipboard read, file dialogs, shell, or arbitrary filesystem access.
  Mobile builds add only the barcode scanner permission needed for QR pairing.

## Status (honest)

**Implemented & working:** envelope encryption (Argon2id + XChaCha20-Poly1305 per
field), SQLite storage with encrypted secret fields, projects/services from 16
templates, reveal/copy one secret at a time (the list/view path never decrypts —
strength is precomputed), real RFC-6238 TOTP, clipboard copy with configurable
auto-clear, create project / add / remove service that persist, drag-reorder,
per-project icons (size-capped), search/sort/filter, lock/unlock, first-run setup
with backend-enforced password policy and opt-in example data, offline brand marks
(monogram/glyph, no network), expiration metadata, editable services, encrypted
last-3 password archive, Personal/global vault, and local Android QR pairing
foundation with encrypted LAN vault transfer.

**Planned (shown in the UI as PLANNED / PREVIEW, not yet wired):** SQLCipher whole-DB
encryption, encrypted file attachments (drops currently record metadata only),
inactivity auto-lock timer, Windows Hello / OS-keychain unlock, breach monitoring,
cloud sync, conflict merge UI, erase-all, password recovery (there is intentionally
**no** recovery — the master password is the only key), and iOS packaging.

## Launch (Windows)

Double-click `gui.bat`, or:

```bat
gui.bat
```

It runs `npm run gui` → `tauri dev`, which serves the frontend on
`http://localhost:1420` and opens the native desktop window. On first run you'll be
walked through creating a master password; the vault is then seeded with example
content so you can see it working.

> **Run the Tauri desktop app from Windows, not WSL.** Tauri builds the Windows
> app (WebView2) and does not cross-compile from WSL. Frontend (`npm run dev`) and the
> Rust vault-core tests run fine in WSL; the desktop window must be launched on the
> Windows side.

### Windows prerequisites

- Node.js + npm
- Rust via rustup (`rustup default stable-msvc`)
- Microsoft C++ Build Tools ("Desktop development with C++")
- WebView2 Runtime (bundled on current Windows; otherwise the Evergreen Bootstrapper)

## Checks

Frontend from the same Windows/npm environment used by the app:

```bash
npm run build    # tsc + vite build
```

Rust vault core (Windows/MSVC):

```bash
cd src-tauri
cargo fmt
cargo test        # crypto round-trip, envelope unwrap, TOTP, strength
cargo clippy
```

Full project check from Windows:

```bat
npm run check
```

## Android QR pairing

The app now has the desktop-side QR flow and the mobile-side scan/import command
path. Desktop Settings → **Devices & pairing** starts a two-minute local pairing
session, displays a QR code and verification code, waits for the phone, then
requires explicit desktop approval before sending the encrypted vault snapshot.

Android setup is npm-only. The Android toolchain on this machine is configured
with:

- `JAVA_HOME=C:\Program Files\Eclipse Adoptium\jdk-21.0.11.10-hotspot`
- `ANDROID_HOME=%LOCALAPPDATA%\Android\Sdk`
- `NDK_HOME=%LOCALAPPDATA%\Android\Sdk\ndk\27.0.12077973`

After changing environment variables, restart the terminal/IDE so Windows picks
up the new values. The Android npm scripts also run through
`scripts\android-env.bat`, which forces these paths for the command being run.

```bat
npm run mobile:android:init
npm run mobile:android:dev
npm run mobile:android:build
npm run mobile:android:build:phone
```

`mobile:android:build` produces the unsigned release APK and AAB here:

```text
src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk
src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab
```

`mobile:android:build:phone` builds a smaller signed arm64 APK for modern Android
phones:

```text
src-tauri/gen/android/app/build/outputs/apk/arm64/release/app-arm64-release.apk
```

Android release signing is scaffolded but secret-free. Create a private
`src-tauri/gen/android/keystore.properties` from
`src-tauri/gen/android/keystore.properties.example`; when that file is present,
Gradle signs release builds automatically. Keep the `.jks` and
`keystore.properties` private.

The LAN QR pairing host is detected from the local network interface. If Windows
chooses the wrong adapter, set `MONOLITH_PAIR_HOST` to the desktop LAN IP before
starting the app.

## Agent-friendly credential import

MONOLITH can import a local JSON bundle from `Settings -> Agent Import`. This is
for AI agents or scripts that scan your private credential notes and prepare a
structured bundle without printing secret values in chat. The import command
encrypts values immediately, imports globals into `Personal` by default, and
upserts by `project + templateId + label` so repeated imports update existing
services and archive replaced secrets instead of duplicating rows.

See [docs/AGENT_IMPORT.md](docs/AGENT_IMPORT.md) and
[docs/agent-import.schema.json](docs/agent-import.schema.json). Plaintext import
files should be deleted after import and are ignored by git when named
`*.monolith-import.json` or `monolith-import*.json`.

## Windows releases and updates

Desktop updates use Tauri's signed updater with a GitHub Release manifest:

```text
https://github.com/designer9999/monolith/releases/latest/download/latest.json
```

The updater signing private key lives at `.tauri/monolith.key` on the release
machine and is intentionally ignored by git. Do not publish it and do not lose
it; existing installs can only update to artifacts signed by the same key.

```bat
npm run release:set-version -- 0.1.1
npm run release:windows
npm run release:github
```

`release:windows` builds a signed NSIS installer, its `.sig`, and `latest.json`
under `release\vX.Y.Z`. `release:github` also uploads those files to the GitHub
release.

Updates replace application binaries only. The encrypted vault and settings live
in the OS app-data directory (`monolith.vault.db`) and are preserved across app
updates. The installer/uninstaller is not wired to delete app-data; data wipe is
an explicit in-app action, not part of update or uninstall flows.

> Building the full Tauri binary in WSL pulls a Linux-only `dbus` system dependency
> (`libdbus-1-dev` + `pkg-config`). It isn't needed on Windows. To compile the whole
> backend in WSL, install those packages; otherwise build on Windows.

## Project layout

```
src/                      React + TypeScript frontend (Tailwind-only styling)
  lib/                    tauri.ts (typed command wrappers), types.ts, icons, ui, format
  components/             views (Home, ProjectView, Browser, Settings, …) + ui/ primitives
src-tauri/src/
  commands.rs             the narrow Tauri command surface
  state.rs                managed state: DB connection + unlocked vault key (Mutex)
  vault/                  crypto.rs (Argon2id + XChaCha20-Poly1305), envelope model
  db/                     schema + repositories (field-level encryption)
  templates.rs            built-in service templates (Supabase, GitHub, …)
  totp.rs                 RFC 6238 TOTP
  seed.rs                 first-run demo content
```
