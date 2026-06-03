# MONOLITH Agent Import

MONOLITH accepts a local JSON bundle that another agent or script can generate from private credential notes. The bundle is pasted, selected, or dropped into `Settings -> Agent Import` while the vault is unlocked. Secret values are encrypted immediately by the same Rust path used by manual service creation and edits.

Do not print secret values in chat. Do not commit generated import bundles. Generated files named `*.monolith-import.json` or `monolith-import*.json` are git-ignored.

## AI Agent Workflow

This is the intended workflow when another local AI agent is helping collect credentials:

1. Open MONOLITH and unlock the vault.
2. Go to `Settings -> Agent Import`.
3. Copy the in-app agent handoff prompt.
4. Replace the placeholder folder paths with the local credential folders that the agent may read.
5. Ask the agent to write one `*.monolith-import.json` file that matches `docs/agent-import.schema.json`.
6. Drop or select that file in MONOLITH. Import starts immediately.
7. Delete the plaintext import file after import.

The generated bundle is a temporary bridge. It is not the vault. Secrets only become protected after MONOLITH encrypts them into the local database.

## Copyable Agent Prompt

Use this prompt when asking another local agent to prepare an import:

```text
You are preparing a MONOLITH agent import bundle.

Read only these local credential folders or files:
- <paste credential folder path 1>
- <paste credential folder path 2>

Do not print, summarize, or expose secret values in chat. Produce one JSON file that matches docs/agent-import.schema.json. Use version 1.

Put global and personal accounts under defaultProjectName "Personal". Put project-specific credentials under projectName only when the file clearly names a project.

Use stable labels, because MONOLITH upserts by project + templateId + label. Re-running the same import should update existing services, not create duplicates.

Use templateId values such as github, apple, mega, topaz, huggingface, instagram, login, zeroid, openai, vercel, supabase, postgres, ssh, card, domain, and note. Use note for anything that does not fit a template yet.

Use expiresAt only when a real expiration, renewal, or planned rotation date is present, formatted YYYY-MM-DD. Do not invent dates.

Save the result as monolith-import.monolith-import.json. Do not commit the file. After import, delete the plaintext bundle.
```

## Import Flow

1. Generate a bundle, for example `local.monolith-import.json`.
2. Open MONOLITH and unlock the vault.
3. Go to `Settings -> Agent Import`.
4. Drop/select the JSON file, or paste the full JSON and press `Import JSON`.
5. Check the counts. `NEW` means created, `UPDATED` means matched existing service and archived replaced secrets, `SKIPPED` means empty item.
6. Delete the plaintext import file after the import is done.

## Direct Local Import

For local automation, MONOLITH also has a Rust CLI importer that uses the same
encrypted import pipeline as the Settings screen. It requires a remembered local
unlock, so install/run MONOLITH, unlock once, then run:

```powershell
cd C:\Radionica\02_Development\Rust\Tauri\PassManager\src-tauri
cargo run --bin monolith_agent_import -- ..\local.monolith-import.json
```

The CLI prints only counts and redacted item errors. It never prints imported
secret values.

## Bundle Shape

```json
{
  "version": 1,
  "source": "local credential folders",
  "defaultProjectName": "Personal",
  "items": [
    {
      "templateId": "github",
      "label": "personal",
      "env": "all",
      "fields": [
        { "label": "Username", "value": "example-user" },
        { "label": "Personal Access Token", "value": "ghp_example" }
      ]
    }
  ]
}
```

`defaultProjectName` is optional. If no project is supplied, MONOLITH imports into `Personal`. If `projectName` is supplied and does not exist, MONOLITH creates that project. If `projectId` is supplied, it must already exist.

## Template IDs

Use these `templateId` values:

`supabase`, `google`, `github`, `vercel`, `stripe`, `cloudflare`, `aws`, `openai`, `postgres`, `shopify`, `smtp`, `ssh`, `login`, `apple`, `mega`, `topaz`, `huggingface`, `instagram`, `domain`, `card`, `note`, `prisma`, `claude`, `resend`, `runpod`, `zeroid`.

Field labels must match the selected template. Useful common mappings:

- `login`: `URL`, `Email / Username`, `Password`
- `apple`: `Account Email`, `Password`, `Recovery Email`, `Trusted Phone`, `Recovery Key`, `Backup Codes`
- `mega`: `Account Email`, `Password`, `Recovery Key`, `Notes`
- `topaz`: `Account Email`, `Password`, `License Key`, `Notes`
- `huggingface`: `Username`, `Account Email`, `Access Token`, `Organization`
- `instagram`: `Username`, `Account Email`, `Password`, `Recovery Email`, `Phone`, `Backup Codes`
- `github`: `Username`, `Account Email`, `Personal Access Token`, `SSH Private Key`, `Webhook Secret`, `OAuth Client ID`, `OAuth Secret`
- `zeroid`: `Client ID`, `Client Secret`, `Issuer URL`, `Account Email`
- `openai`: `API Key`, `Organization ID`, `Project ID`
- `vercel`: `Account Email`, `Access Token`, `Team ID`, `Project ID`, `Deploy Hook URL`
- `note`: `Note`

Use `note` for credentials that do not fit a template yet. Put the whole private note in the `Note` field.

## Upsert Rules

MONOLITH matches existing services by:

```text
project + templateId + label
```

If a match exists, the service is updated. Changed secret values are archived in password history, keeping the latest three previous values per field. If no match exists, a new service is created.

If `label` is omitted, MONOLITH derives one from non-secret fields such as email, username, URL, host, client ID, project ID, or account ID. It never derives labels from password/API-key fields.

## Dates

Use `expiresAt` only for real expiration, renewal, or planned rotation dates:

```json
{ "expiresAt": "2026-08-31" }
```

Do not invent expiration dates.
