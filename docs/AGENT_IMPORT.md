# MONOLITH Agent Import

MONOLITH accepts a local JSON bundle that another agent or script can generate from private credential notes. The bundle is pasted into `Settings -> Agent Import` while the vault is unlocked. Secret values are encrypted immediately by the same Rust path used by manual service creation and edits.

Do not print secret values in chat. Do not commit generated import bundles. Generated files named `*.monolith-import.json` or `monolith-import*.json` are git-ignored.

## Agent Prompt

Use this prompt when asking another local agent to prepare an import:

```text
Read these local credential folders only:
- C:\Radionica\07_Private\Credentials\INFO
- C:\Radionica\Scamlitics\Logins

Do not print, summarize, or expose secret values in chat. Produce one JSON file that matches docs/agent-import.schema.json. Put global/personal accounts such as Gmail, Instagram, ZeroID, personal GitHub, personal API keys, and standalone logins under defaultProjectName "Personal". Put project-specific credentials under their projectName when the file clearly names a project.

Use stable labels, because MONOLITH upserts by project + templateId + label. Re-running the import should update the same service, not create duplicates. Use expiresAt only when a real expiration or rotation date is present, formatted YYYY-MM-DD.
```

## Import Flow

1. Generate a bundle, for example `local.monolith-import.json`.
2. Open MONOLITH and unlock the vault.
3. Go to `Settings -> Agent Import`.
4. Paste the full JSON and press `Import JSON`.
5. Check the counts. `NEW` means created, `UPDATED` means matched existing service and archived replaced secrets, `SKIPPED` means empty item.
6. Delete the plaintext import file after the import is done.

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

`supabase`, `google`, `github`, `vercel`, `stripe`, `cloudflare`, `aws`, `openai`, `postgres`, `shopify`, `smtp`, `ssh`, `login`, `domain`, `card`, `note`, `prisma`, `claude`, `resend`, `runpod`, `zeroid`.

Field labels must match the selected template. Useful common mappings:

- `login`: `URL`, `Email / Username`, `Password`
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
