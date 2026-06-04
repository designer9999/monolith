# Agent Kickoff: Rust + Tauri + Vite + shadcn/ui

> Reusable starting brief for AI coding agents before creating or changing any project that uses **Rust + Tauri + Vite + shadcn/ui**.
>
> Environment assumption: **Windows host + WSL development shell + tmux agent sessions**.
>
> This file is intentionally **future-proof**. Do not hard-code today’s major versions as permanent truth. Before coding, confirm the latest official docs and adapt the plan.

---

## 1. Mission for the agent

You are starting a new desktop/webview app stack using:

- Rust for native/backend logic.
- Tauri for the desktop shell and OS integration.
- Vite for frontend dev server and production build.
- React + TypeScript unless the project says otherwise.
- shadcn/ui for copy-in UI components.
- Tailwind CSS latest as required by shadcn/ui setup v4.3.
- npm as the default JavaScript package manager unless the project already uses another one.

Your first job is **not** to immediately scaffold code.

Your first job is to:

1. Detect the latest stable docs and recommended setup paths.
2. Identify current breaking changes, major versions, and migration notes.
3. Produce a short implementation plan.
4. Only then scaffold or modify the project.

Prefer the current official guidance over assumptions in this file.

---

## 2. Required documentation check before coding

Before writing code, search the web and read the current official latest documentation.

### Rust / Cargo

Check:

- Rust install/get-started docs.
- Cargo Book.
- Rust release notes if the Rust edition or MSRV matters.

Search examples:

```txt
Rust official install rustup stable Cargo getting started
Rust latest stable release edition MSRV Cargo docs
```

Questions to answer:

- What is the current stable Rust toolchain?
- Is there a newer Rust edition relevant to a new project?
- Are there current Cargo recommendations for workspace layout, linting, or resolver settings?

### Tauri

Check:

- Latest Tauri docs landing page.
- Latest create-project guide.
- Latest Vite integration guide.
- Latest permissions/capabilities docs.
- Latest plugin docs only if a plugin is needed.

Search examples:

```txt
Tauri latest create project Vite Rust official docs
Tauri latest permissions capabilities commands official docs
Tauri latest Windows WSL development official docs
```

Questions to answer:

- What is the latest stable Tauri major version?
- What is the current recommended project creation command?
- What changed in `tauri.conf.*`, capabilities, permissions, plugins, commands, or build config?
- Are there Windows-specific or WSL-specific warnings?

### Vite

Check:

- Vite Getting Started.
- Vite React + TypeScript template docs.
- Current Node.js minimum version.
- Current dev server, HMR, build, and config recommendations.

Search examples:

```txt
Vite latest getting started React TypeScript npm official docs
Vite latest Node version requirement config server hmr official docs
```

Questions to answer:

- What is the latest stable Vite major version?
- What Node.js versions are supported?
- Has the recommended React plugin, build engine, config format, or template command changed?

### shadcn/ui

Check:

- shadcn/ui installation overview.
- shadcn/ui Vite guide.
- shadcn/ui changelog.
- Current Tailwind CSS setup expected by shadcn/ui.
- Current `components.json` format and CLI command names.

Search examples:

```txt
shadcn/ui latest Vite installation Tailwind official docs
shadcn/ui latest changelog CLI components.json official docs
shadcn create Vite React TypeScript Tailwind latest
```

Questions to answer:

- What is the current recommended install path: `shadcn`, `shadcn@latest`, `shadcn/create`, or something else?
- What Tailwind version/setup is expected?
- Did aliases, registry behavior, component installation, theming, icons, or CSS variables change?

---

## 3. Output required before scaffold

Before making files, produce a compact plan in this shape:

```md
## Current stack check

- Rust: <latest stable / edition / notable requirement>
- Tauri: <latest major + recommended command>
- Vite: <latest major + Node requirement>
- shadcn/ui: <latest CLI/setup path + Tailwind requirement>
- Windows/WSL notes: <important constraints>

## Proposed setup

- Package manager: <pnpm/npm/yarn/bun>
- Frontend: <React + TypeScript unless changed>
- Desktop shell: <Tauri latest stable>
- UI: <shadcn/ui + Tailwind current setup>
- Validation commands: <commands to run after scaffold>

## Risks / things to verify

- <anything that may break due to versions, OS, WSL, or Tauri permissions>
```

Do not proceed with large implementation until this plan is clear.

---

## 4. Windows + WSL + tmux assumptions

The developer works on **Windows**, but often starts agents inside **WSL** and manages them with **tmux**.

Follow these rules:

### Prefer WSL for development commands

Run project commands from WSL unless there is a specific reason to run in native Windows PowerShell.

Use:

```bash
pwd
uname -a
node --version
npm --version
rustc --version
cargo --version
```

Confirm the repo is not being edited from a problematic mount if performance matters.

Preferred location:

```txt
~/code/<project-name>
```

Avoid heavy builds under `/mnt/c/...` unless the user intentionally keeps repos there.

### Remember Tauri is a desktop app

Tauri launches a native desktop window. In WSL this can be tricky depending on WSLg, Windows WebView2, GPU/display setup, and whether the Tauri CLI expects Windows-native dependencies.

Before assuming `npm run tauri -- dev` will work inside WSL, verify the latest Tauri docs for Windows + WSL. If unclear, separate workflows:

```txt
Frontend dev in WSL:
  npm run dev

Rust checks in WSL:
  cd src-tauri && cargo check && cargo test

Full Tauri desktop launch:
  use the workflow recommended by current Tauri docs for Windows/WSL
```

If the Tauri dev window fails in WSL, do not randomly patch config. First identify whether it is a WSL display/WebView/build-tooling issue.

---

## 5. Preferred project creation decision tree

After checking the latest docs, choose one path.

### Path A: Official Tauri creator

Use this when starting a completely new desktop app and the official Tauri creator supports the desired frontend stack well.

Generic command pattern:

```bash
<pkg-manager> create tauri-app
```

Then choose the current recommended options for:

```txt
language: TypeScript
frontend framework: React
package manager: npm
```

Do not assume old prompt names are still correct. Follow the latest creator prompts.

### Path B: Vite first, then Tauri

Use this when the frontend layout needs custom control or the official Tauri creator lags behind current Vite/shadcn practices.

Generic command pattern:

```bash
npm create vite@latest <project-name> -- --template react-ts
cd <project-name>
npm install
# add/init Tauri using current official command
# add/init shadcn/ui using current official command
```

Replace the Tauri and shadcn commands with the current official commands discovered during the docs check.

### Path C: Existing project

Use this when the repo already exists.

Do not overwrite config blindly. Inspect first:

```bash
ls
cat package.json
find . -maxdepth 3 -type f \( -name 'vite.config.*' -o -name 'tauri.conf.*' -o -name 'components.json' -o -name 'Cargo.toml' \)
```

Then make minimal changes.

---

Rules:

- Keep shadcn primitives and componets in `components/ui`.
- Only use componets user always will ask for varients additional varients as projcet goes.
- Keep native functionality behind narrow Tauri commands.
- Keep frontend wrappers in `src/lib/tauri.ts` or equivalent.
- Do not mix large business logic into UI components.

---

## 7. Tauri command design rules

When adding Rust commands:

- Keep commands small.
- Validate all input in Rust, not only in the frontend.
- Return typed results.
- Avoid exposing broad filesystem, shell, or network powers.
- Add permissions/capabilities only when required.
- Prefer explicit allowlists.
- Keep secrets out of the frontend bundle.

Good command shape:

```rust
#[tauri::command]
pub fn example(input: String) -> Result<String, String> {
    let input = input.trim();

    if input.is_empty() {
        return Err("Input is required".into());
    }

    Ok(format!("Received: {input}"))
}
```

---

## 8. Vite config rules

Before editing `vite.config.*`, check the latest Vite and Tauri docs.

Common concerns:

- Tauri dev server URL.
- Strict port behavior.
- HMR host when using WSL, LAN, or mobile targets.
- Watching/ignoring `src-tauri`.
- Path aliases.
- React plugin setup.
- TypeScript config split.

Vite major versions may change default behavior.

---

## 9. shadcn/ui rules

Before installing components:

1. Confirm the current install command.
2. Confirm whether Tailwind setup is automatic or manual.
3. Confirm whether the CLI writes `components.json`.
4. Confirm alias names.
5. Confirm where global CSS should live.
6. Confirm current icon package and utility dependencies.

Component policy:

- Add only the components needed for the first screen.
- Commit generated component files.
- Treat generated shadcn files as source code that can be edited.
- Prefer composition over deep rewrites of primitives.
- Keep design tokens centralized in the current recommended CSS/theme location.

Common first components, only if needed:

```bash
button card input label textarea dialog dropdown-menu sonner
```

Use the current CLI syntax discovered from docs.

## CSS Organization

Keep CSS separated by purpose. Do not overload one huge global CSS file.

Use separate files for reusable areas like motion, utilities, loaders, and component-specific styles.

Example:

````css
@import '../components/ui/shimmering-loader/shimmering-loader.css';
@import '../styles/utilities.css';
@import '../styles/motion.css';

---

## 10. Validation commands

After scaffold or changes, run the commands that match the current package manager and project structure.

Likely commands:

```bash
npm install
npm run build
npm run dev
````

For Tauri:

```bash
npm run tauri -- dev
npm run tauri -- build
```

For Rust:

```bash
cd src-tauri
cargo check
cargo fmt --check
cargo clippy
cargo test
```

If a command fails because of WSL/Windows/Tauri desktop integration, document the exact failure and separate frontend/Rust validation from native desktop launch validation.

---

## 11. Security checklist

Before adding native APIs, answer:

```txt
1. Does this feature really need native desktop access?
2. What is the smallest permission/capability that enables it?
3. What untrusted input crosses from frontend to Rust?
4. What errors can happen and how are they returned?
5. Are any secrets exposed to frontend code?
6. Can the feature be tested without broad OS permissions?
```

Never add broad permissions just to make something work quickly.

---

## 12. First useful app baseline

After every edit, inspect what the edit touched across the project. Update all affected imports, paths, routes, exports, types, tests, configs, scripts, and docs.

Never leave duplicated code, old files, stale paths, unused imports, obsolete comments, dead code, temporary debug code, or abandoned implementations behind.

Prefer real root-cause fixes over masking problems with fallbacks, silent defaults, broad try/catch blocks, mock data, or compatibility layers. Test and understand what actually works instead of bloating the code with defensive branches.

Before adding new code, check for existing project patterns and reuse them. Keep changes small, direct, and maintainable.

Before finishing, search for old names, old paths, removed APIs, and replaced commands. Verify with the most relevant checks, such as typecheck, lint, tests, build, `cargo check`, or Tauri dev/build commands.

.

Add these only when the actual project requires them.

---

## 13. Minimal agent prompt to paste with this file

```txt
Read AGENT_KICKOFF_RUST_TAURI_VITE_SHADCN.md first.

Before coding, search current official docs for Rust/Cargo, Tauri, Vite, and shadcn/ui. Confirm latest stable major versions, install commands, Node/Rust requirements, Tailwind/shadcn setup, and Windows + WSL constraints.

Then produce the required "Current stack check" and "Proposed setup" sections. After that, scaffold the smallest working starter app: React + TypeScript frontend, Tauri desktop shell, shadcn/ui baseline, one safe Rust command round trip, and validation commands.

I am on Windows and usually run agents in WSL inside tmux. Keep commands WSL-friendly and call out anything that must be run from Windows instead.
```
