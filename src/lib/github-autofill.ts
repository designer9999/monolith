export interface GitHubTokenProfile {
  login: string;
  name?: string;
  email?: string;
  scopes: string[];
}

interface GitHubUserResponse {
  login?: string;
  name?: string | null;
  email?: string | null;
  message?: string;
}

interface GitHubEmailResponse {
  email?: string;
  primary?: boolean;
  verified?: boolean;
  visibility?: string | null;
}

const API_ROOT = "https://api.github.com";
const API_HEADERS = {
  Accept: "application/vnd.github+json",
  "X-GitHub-Api-Version": "2022-11-28",
};

function authHeaders(token: string): HeadersInit {
  return {
    ...API_HEADERS,
    Authorization: `Bearer ${token}`,
  };
}

function parseScopes(headers: Headers): string[] {
  const raw = headers.get("x-oauth-scopes") ?? "";
  return raw
    .split(",")
    .map((scope) => scope.trim())
    .filter(Boolean);
}

async function githubJson<T>(path: string, token: string): Promise<{ data: T; headers: Headers }> {
  const response = await fetch(`${API_ROOT}${path}`, {
    method: "GET",
    headers: authHeaders(token),
  });
  const data = (await response.json().catch(() => ({}))) as T & { message?: string };
  if (!response.ok) {
    if (response.status === 401) {
      throw new Error("Token was rejected by GitHub. Paste the full active token.");
    }
    throw new Error(data.message || `GitHub request failed with ${response.status}.`);
  }
  return { data, headers: response.headers };
}

async function fetchPrimaryEmail(token: string): Promise<string | undefined> {
  try {
    const { data } = await githubJson<GitHubEmailResponse[]>("/user/emails", token);
    if (!Array.isArray(data)) return undefined;
    return (
      data.find((email) => email.primary && email.verified)?.email ??
      data.find((email) => email.verified && email.visibility === "public")?.email ??
      data.find((email) => email.verified)?.email
    );
  } catch {
    return undefined;
  }
}

export async function fetchGitHubTokenProfile(token: string): Promise<GitHubTokenProfile> {
  const clean = token.trim();
  if (!clean) {
    throw new Error("Paste a GitHub token first.");
  }

  const { data, headers } = await githubJson<GitHubUserResponse>("/user", clean);
  if (!data.login) {
    throw new Error("GitHub did not return an account for this token.");
  }

  return {
    login: data.login,
    name: data.name ?? undefined,
    email: data.email ?? (await fetchPrimaryEmail(clean)),
    scopes: parseScopes(headers),
  };
}
