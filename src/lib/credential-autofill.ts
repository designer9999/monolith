import { fetchGitHubTokenProfile } from "./github-autofill";

type FieldType = "text" | "password" | "api_key" | "url" | "email" | "json";

export interface CredentialAutofillResult {
  label?: string;
  values?: Record<string, string>;
  message: string;
  note?: string;
}

interface CredentialAssist {
  buttonLabel: string;
  idleText: string;
}

interface VercelUserResponse {
  user?: {
    id?: string;
    username?: string;
    email?: string;
    name?: string | null;
    defaultTeamId?: string | null;
  };
  error?: { message?: string };
}

interface CloudflareVerifyResponse {
  success?: boolean;
  errors?: { message?: string }[];
  result?: { id?: string; status?: string };
}

interface CloudflareAccountsResponse {
  success?: boolean;
  result?: { id?: string; name?: string }[];
}

interface OpenAIModelsResponse {
  data?: unknown[];
  error?: { message?: string };
}

interface AnthropicModelsResponse {
  data?: unknown[];
  error?: { message?: string };
}

interface StripeAccountResponse {
  id?: string;
  email?: string;
  business_profile?: { name?: string | null };
  error?: { message?: string };
}

const SECRET_UNAVAILABLE =
  "Provider APIs can verify tokens and return account metadata, but they cannot recover already-created secret values.";

export function credentialAssistFor(templateId: string, fieldLabel: string): CredentialAssist | null {
  if (templateId === "github" && fieldLabel === "Personal Access Token") {
    return {
      buttonLabel: "Fetch GitHub account",
      idleText: "Paste a full GitHub token. Fetch fills username/email only.",
    };
  }
  if (templateId === "vercel" && fieldLabel === "Access Token") {
    return {
      buttonLabel: "Fetch Vercel account",
      idleText: "Paste a Vercel token. Fetch fills account email and team when available.",
    };
  }
  if (templateId === "cloudflare" && fieldLabel === "API Token") {
    return {
      buttonLabel: "Verify Cloudflare token",
      idleText: "Paste a Cloudflare API token. Fetch verifies it and fills one account ID when possible.",
    };
  }
  if (templateId === "stripe" && ["Secret Key", "Restricted Key"].includes(fieldLabel)) {
    return {
      buttonLabel: "Verify Stripe key",
      idleText: "Paste a Stripe key. Fetch verifies the account and mode.",
    };
  }
  if (templateId === "openai" && fieldLabel === "API Key") {
    return {
      buttonLabel: "Verify OpenAI key",
      idleText: "Paste an OpenAI API key. Fetch verifies access.",
    };
  }
  if (templateId === "claude" && fieldLabel === "API Key") {
    return {
      buttonLabel: "Verify Claude key",
      idleText: "Paste an Anthropic key. Fetch verifies access.",
    };
  }
  if (templateId === "supabase" && ["Anon Key", "Service Role Key"].includes(fieldLabel)) {
    return {
      buttonLabel: "Inspect JWT",
      idleText: "Paste a Supabase JWT. Inspect reads role/expiry locally without contacting Supabase.",
    };
  }
  return null;
}

export function credentialFieldHint(
  templateId: string,
  fieldLabel: string,
  fieldType: FieldType,
  secret: boolean,
): string | undefined {
  if (credentialAssistFor(templateId, fieldLabel)) return "paste token · fetch below";
  const label = fieldLabel.toLowerCase();
  if (fieldType === "api_key" || label.includes("token") || label.includes("api key")) {
    return secret ? "paste token" : "paste public key";
  }
  if (label.includes("private key")) return "paste private key";
  if (label.includes("webhook secret")) return "paste signing secret";
  if (label.includes("oauth secret") || label.includes("client secret")) return "paste secret";
  return undefined;
}

export function credentialPlaceholder(
  fieldLabel: string,
  fieldType: FieldType,
  secret: boolean,
  area: boolean,
  hasExistingValue = false,
): string {
  if (hasExistingValue && secret) {
    if (fieldType === "api_key" || /token|api key/i.test(fieldLabel)) {
      return "leave unchanged or paste replacement token";
    }
    return "leave unchanged or paste replacement";
  }
  if (area && /private key/i.test(fieldLabel)) return "paste private key";
  if (fieldType === "api_key" || /token|api key/i.test(fieldLabel)) return "paste token or API key";
  if (fieldType === "password" || /password/i.test(fieldLabel)) return "enter password";
  if (fieldType === "email") return "enter email";
  if (fieldType === "url") return "enter URL";
  if (fieldType === "json") return "paste JSON";
  return `enter ${fieldLabel.toLowerCase()}`;
}

export async function runCredentialAutofill(
  templateId: string,
  fieldLabel: string,
  token: string,
): Promise<CredentialAutofillResult> {
  const clean = token.trim();
  if (!clean) throw new Error("Paste the token first.");

  if (templateId === "github" && fieldLabel === "Personal Access Token") {
    const profile = await fetchGitHubTokenProfile(clean);
    return {
      label: profile.login,
      values: {
        Username: profile.login,
        ...(profile.email ? { "Account Email": profile.email } : {}),
      },
      message: `Fetched ${profile.login}${profile.scopes.length ? ` · ${profile.scopes.length} scopes` : ""}`,
      note: SECRET_UNAVAILABLE,
    };
  }

  if (templateId === "vercel" && fieldLabel === "Access Token") {
    return fetchVercel(clean);
  }
  if (templateId === "cloudflare" && fieldLabel === "API Token") {
    return fetchCloudflare(clean);
  }
  if (templateId === "stripe" && ["Secret Key", "Restricted Key"].includes(fieldLabel)) {
    return fetchStripe(clean);
  }
  if (templateId === "openai" && fieldLabel === "API Key") {
    return fetchOpenAI(clean);
  }
  if (templateId === "claude" && fieldLabel === "API Key") {
    return fetchAnthropic(clean);
  }
  if (templateId === "supabase" && ["Anon Key", "Service Role Key"].includes(fieldLabel)) {
    return inspectJwt(clean, "Supabase");
  }

  throw new Error("No fetcher is available for this token type yet.");
}

async function fetchVercel(token: string): Promise<CredentialAutofillResult> {
  const data = await fetchJson<VercelUserResponse>("https://api.vercel.com/v2/user", {
    headers: { Authorization: `Bearer ${token}` },
  });
  const user = data.user;
  if (!user?.email && !user?.username) throw new Error("Vercel did not return an account.");
  return {
    label: user.username || user.email,
    values: {
      ...(user.email ? { "Account Email": user.email } : {}),
      ...(user.defaultTeamId ? { "Team ID": user.defaultTeamId } : {}),
    },
    message: `Fetched ${user.username || user.email}`,
    note: "Vercel does not expose project IDs or deploy hook URLs from only a token.",
  };
}

async function fetchCloudflare(token: string): Promise<CredentialAutofillResult> {
  const verified = await fetchJson<CloudflareVerifyResponse>(
    "https://api.cloudflare.com/client/v4/user/tokens/verify",
    { headers: { Authorization: `Bearer ${token}` } },
  );
  if (!verified.success) {
    throw new Error(verified.errors?.[0]?.message || "Cloudflare rejected this token.");
  }

  let accountId: string | undefined;
  try {
    const accounts = await fetchJson<CloudflareAccountsResponse>(
      "https://api.cloudflare.com/client/v4/accounts",
      { headers: { Authorization: `Bearer ${token}` } },
    );
    const result = accounts.result?.filter((account) => account.id) ?? [];
    if (result.length === 1) accountId = result[0].id;
  } catch {
    // Account listing needs extra permissions; token verification is still useful.
  }

  return {
    values: accountId ? { "Account ID": accountId } : undefined,
    message: `Verified Cloudflare token${verified.result?.status ? ` · ${verified.result.status}` : ""}`,
    note: accountId
      ? "Filled the only visible account ID."
      : "Account ID requires a token that can list accounts, or you can paste it manually.",
  };
}

async function fetchStripe(token: string): Promise<CredentialAutofillResult> {
  const data = await fetchJson<StripeAccountResponse>("https://api.stripe.com/v1/account", {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!data.id) throw new Error(data.error?.message || "Stripe did not return an account.");
  const mode = token.startsWith("sk_live_") || token.startsWith("rk_live_") ? "live" : "test";
  return {
    values: { Mode: mode },
    message: `Verified Stripe account ${data.id}`,
    note: "Stripe does not expose webhook signing secrets from the account API.",
  };
}

async function fetchOpenAI(token: string): Promise<CredentialAutofillResult> {
  const data = await fetchJson<OpenAIModelsResponse>("https://api.openai.com/v1/models", {
    headers: { Authorization: `Bearer ${token}` },
  });
  return {
    message: `Verified OpenAI key${Array.isArray(data.data) ? ` · ${data.data.length} models visible` : ""}`,
    note: "OpenAI keys do not expose organization or project IDs from this verification call.",
  };
}

async function fetchAnthropic(token: string): Promise<CredentialAutofillResult> {
  const data = await fetchJson<AnthropicModelsResponse>(
    "https://api.anthropic.com/v1/models?limit=1",
    {
      headers: {
        "x-api-key": token,
        "anthropic-version": "2023-06-01",
      },
    },
  );
  return {
    message: `Verified Anthropic key${Array.isArray(data.data) ? " · models visible" : ""}`,
    note: "Anthropic keys do not expose workspace or organization IDs from this verification call.",
  };
}

function inspectJwt(token: string, provider: string): CredentialAutofillResult {
  const parts = token.split(".");
  if (parts.length < 2) throw new Error("This does not look like a JWT.");
  const payload = JSON.parse(decodeBase64Url(parts[1])) as {
    role?: string;
    exp?: number;
    iss?: string;
    ref?: string;
  };
  const exp = payload.exp ? new Date(payload.exp * 1000).toISOString().slice(0, 10) : undefined;
  return {
    message: `${provider} JWT${payload.role ? ` · ${payload.role}` : ""}${exp ? ` · expires ${exp}` : ""}`,
    note: payload.ref ? `Project ref: ${payload.ref}` : "JWT inspection is local. It does not verify server-side permissions.",
  };
}

async function fetchJson<T>(url: string, init: RequestInit): Promise<T> {
  const response = await fetch(url, { method: "GET", ...init });
  const data = (await response.json().catch(() => ({}))) as T & {
    message?: string;
    error?: { message?: string };
    errors?: { message?: string }[];
  };
  if (!response.ok) {
    throw new Error(
      data.error?.message ||
        data.errors?.[0]?.message ||
        data.message ||
        `Provider request failed with ${response.status}.`,
    );
  }
  return data;
}

function decodeBase64Url(value: string): string {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
  return decodeURIComponent(
    Array.from(atob(padded))
      .map((char) => `%${char.charCodeAt(0).toString(16).padStart(2, "0")}`)
      .join(""),
  );
}
