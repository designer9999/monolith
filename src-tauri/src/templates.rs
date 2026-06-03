//! Built-in service templates.
//!
//! A template is the preset shape of a service (Supabase, GitHub, …): the fields
//! it needs, which are secret, which are sensitive, and whether it supports TOTP.
//! Ported directly from the MONOLITH design's `data.js` so the catalog matches
//! the mockup exactly.

use serde::Serialize;

use crate::models::FieldType;

/// A single field definition within a template.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateField {
    pub label: &'static str,
    pub secret: bool,
    pub danger: bool,
    pub area: bool,
    pub field_type: FieldType,
}

impl TemplateField {
    const fn new(label: &'static str, secret: bool) -> Self {
        TemplateField {
            label,
            secret,
            danger: false,
            area: false,
            field_type: FieldType::Text,
        }
    }
    const fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
    const fn area(mut self) -> Self {
        self.area = true;
        self
    }
    const fn ty(mut self, t: FieldType) -> Self {
        self.field_type = t;
        self
    }
}

/// A service template: its identity, brand presentation and field list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub mono: &'static str,
    /// Simple Icons slug for the real brand logo, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<&'static str>,
    /// Fallback glyph name (from the icon set) when there's no brand logo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<&'static str>,
    pub color: &'static str,
    pub totp: bool,
    pub group: &'static str,
    pub fields: Vec<TemplateField>,
}

// Short aliases for terse, readable template definitions below.
use FieldType::{ApiKey, Email, Json, Url};

/// Build the full built-in template catalog. Order matches the design.
pub fn catalog() -> Vec<Template> {
    use TemplateField as F;
    vec![
        Template {
            id: "supabase",
            name: "Supabase",
            mono: "S",
            slug: Some("supabase"),
            icon: None,
            color: "#3ecf8e",
            totp: false,
            group: "Backend",
            fields: vec![
                F::new("Project URL", false).ty(Url),
                F::new("Anon Key", true).ty(ApiKey),
                F::new("Service Role Key", true).danger().ty(ApiKey),
                F::new("JWT Secret", true).danger(),
                F::new("Database Password", true),
                F::new("S3 Access Key", true),
                F::new("S3 Secret Key", true),
            ],
        },
        Template {
            id: "google",
            name: "Google Cloud",
            mono: "G",
            slug: Some("google"),
            icon: None,
            color: "#4285f4",
            totp: true,
            group: "Auth",
            fields: vec![
                F::new("Client ID", false),
                F::new("Client Secret", true).danger(),
                F::new("API Key", true).ty(ApiKey),
                F::new("Service Account JSON", true).area().ty(Json),
                F::new("Account Email", false).ty(Email),
                F::new("Account Password", true),
            ],
        },
        Template {
            id: "github",
            name: "GitHub",
            mono: "GH",
            slug: Some("github"),
            icon: None,
            color: "#e8edf2",
            totp: true,
            group: "Dev",
            fields: vec![
                F::new("Username", false),
                F::new("Account Email", false).ty(Email),
                F::new("Personal Access Token", true).danger().ty(ApiKey),
                F::new("SSH Private Key", true).area(),
                F::new("Webhook Secret", true),
                F::new("OAuth Client ID", false),
                F::new("OAuth Secret", true).danger(),
            ],
        },
        Template {
            id: "vercel",
            name: "Vercel",
            mono: "V",
            slug: Some("vercel"),
            icon: None,
            color: "#f2f2f2",
            totp: false,
            group: "Hosting",
            fields: vec![
                F::new("Account Email", false).ty(Email),
                F::new("Access Token", true).danger().ty(ApiKey),
                F::new("Team ID", false),
                F::new("Project ID", false),
                F::new("Deploy Hook URL", false).ty(Url),
            ],
        },
        Template {
            id: "stripe",
            name: "Stripe",
            mono: "St",
            slug: Some("stripe"),
            icon: None,
            color: "#8a82ff",
            totp: false,
            group: "Payments",
            fields: vec![
                F::new("Publishable Key", false),
                F::new("Secret Key", true).danger().ty(ApiKey),
                F::new("Restricted Key", true).ty(ApiKey),
                F::new("Webhook Secret", true),
                F::new("Mode", false),
            ],
        },
        Template {
            id: "cloudflare",
            name: "Cloudflare",
            mono: "CF",
            slug: Some("cloudflare"),
            icon: None,
            color: "#f6821f",
            totp: true,
            group: "Infra",
            fields: vec![
                F::new("Account Email", false).ty(Email),
                F::new("API Token", true).danger().ty(ApiKey),
                F::new("Global API Key", true).danger(),
                F::new("Zone ID", false),
                F::new("Account ID", false),
            ],
        },
        Template {
            id: "aws",
            name: "AWS",
            mono: "A",
            slug: None,
            icon: Some("globe"),
            color: "#ff9900",
            totp: true,
            group: "Infra",
            fields: vec![
                F::new("Access Key ID", false),
                F::new("Secret Access Key", true).danger().ty(ApiKey),
                F::new("Region", false),
                F::new("Account ID", false),
            ],
        },
        Template {
            id: "openai",
            name: "OpenAI",
            mono: "AI",
            slug: None,
            icon: Some("layers"),
            color: "#cdd3da",
            totp: false,
            group: "AI",
            fields: vec![
                F::new("API Key", true).danger().ty(ApiKey),
                F::new("Organization ID", false),
                F::new("Project ID", false),
            ],
        },
        Template {
            id: "postgres",
            name: "Postgres",
            mono: "P",
            slug: Some("postgresql"),
            icon: None,
            color: "#6aa3d6",
            totp: false,
            group: "Backend",
            fields: vec![
                F::new("Host", false),
                F::new("Port", false),
                F::new("User", false),
                F::new("Password", true).danger(),
                F::new("Database", false),
                F::new("Connection URL", true).danger(),
            ],
        },
        Template {
            id: "shopify",
            name: "Shopify",
            mono: "Sh",
            slug: Some("shopify"),
            icon: None,
            color: "#95bf47",
            totp: true,
            group: "Commerce",
            fields: vec![
                F::new("Store URL", false).ty(Url),
                F::new("Admin Email", false).ty(Email),
                F::new("Password", true),
                F::new("Admin API Token", true).danger().ty(ApiKey),
            ],
        },
        Template {
            id: "smtp",
            name: "Email / SMTP",
            mono: "@",
            slug: None,
            icon: Some("globe"),
            color: "#7fb4ff",
            totp: false,
            group: "Backend",
            fields: vec![
                F::new("Host", false),
                F::new("Port", false),
                F::new("Username", false),
                F::new("Password", true).danger(),
                F::new("From Email", false).ty(Email),
            ],
        },
        Template {
            id: "ssh",
            name: "SSH Key",
            mono: "K",
            slug: None,
            icon: Some("terminal"),
            color: "#c0c4cb",
            totp: false,
            group: "Infra",
            fields: vec![
                F::new("Host", false),
                F::new("User", false),
                F::new("Private Key", true).danger().area(),
                F::new("Passphrase", true).danger(),
            ],
        },
        Template {
            id: "login",
            name: "Login",
            mono: "L",
            slug: None,
            icon: Some("key"),
            color: "#c8ff2e",
            totp: true,
            group: "General",
            fields: vec![
                F::new("URL", false).ty(Url),
                F::new("Email / Username", false).ty(Email),
                F::new("Password", true).danger(),
            ],
        },
        Template {
            id: "apple",
            name: "Apple ID",
            mono: "AP",
            slug: Some("apple"),
            icon: None,
            color: "#f5f5f7",
            totp: false,
            group: "Personal",
            fields: vec![
                F::new("Account Email", false).ty(Email),
                F::new("Password", true).danger(),
                F::new("Recovery Email", false).ty(Email),
                F::new("Trusted Phone", false),
                F::new("Recovery Key", true).danger(),
                F::new("Backup Codes", true).area(),
            ],
        },
        Template {
            id: "mega",
            name: "Mega",
            mono: "M",
            slug: Some("mega"),
            icon: None,
            color: "#d9272e",
            totp: true,
            group: "Personal",
            fields: vec![
                F::new("Account Email", false).ty(Email),
                F::new("Password", true).danger(),
                F::new("Recovery Key", true).danger(),
                F::new("Notes", true).area(),
            ],
        },
        Template {
            id: "topaz",
            name: "Topaz",
            mono: "TZ",
            slug: None,
            icon: Some("gem"),
            color: "#5b9dff",
            totp: false,
            group: "Personal",
            fields: vec![
                F::new("Account Email", false).ty(Email),
                F::new("Password", true).danger(),
                F::new("License Key", true).ty(ApiKey),
                F::new("Notes", true).area(),
            ],
        },
        Template {
            id: "huggingface",
            name: "Hugging Face",
            mono: "HF",
            slug: Some("huggingface"),
            icon: None,
            color: "#ffd21e",
            totp: true,
            group: "AI",
            fields: vec![
                F::new("Username", false),
                F::new("Account Email", false).ty(Email),
                F::new("Access Token", true).danger().ty(ApiKey),
                F::new("Organization", false),
            ],
        },
        Template {
            id: "instagram",
            name: "Instagram",
            mono: "IG",
            slug: Some("instagram"),
            icon: None,
            color: "#ff0069",
            totp: true,
            group: "Personal",
            fields: vec![
                F::new("Username", false),
                F::new("Account Email", false).ty(Email),
                F::new("Password", true).danger(),
                F::new("Recovery Email", false).ty(Email),
                F::new("Phone", false),
                F::new("Backup Codes", true).area(),
            ],
        },
        Template {
            id: "domain",
            name: "Domain",
            mono: "D",
            slug: None,
            icon: Some("globe"),
            color: "#60a5fa",
            totp: false,
            group: "General",
            fields: vec![
                F::new("Registrar", false),
                F::new("Login Email", false).ty(Email),
                F::new("Password", true),
                F::new("EPP / Auth Code", true).danger(),
                F::new("Renewal Date", false),
            ],
        },
        Template {
            id: "card",
            name: "Payment Card",
            mono: "C",
            slug: None,
            icon: Some("card"),
            color: "#cbd2da",
            totp: false,
            group: "Finance",
            fields: vec![
                F::new("Card Number", true).danger(),
                F::new("Expiry", false),
                F::new("CVV", true).danger(),
                F::new("Cardholder", false),
            ],
        },
        Template {
            id: "note",
            name: "Secure Note",
            mono: "N",
            slug: None,
            icon: Some("note"),
            color: "#94a3b8",
            totp: false,
            group: "General",
            // A "Secure Note" must actually be secret — its body is encrypted at rest.
            fields: vec![F::new("Note", true).area()],
        },
        Template {
            id: "prisma",
            name: "Prisma",
            mono: "Pr",
            slug: Some("prisma"),
            icon: None,
            // Prisma's brand hex (#2D3748) is too dark on the near-black tile; use a readable tint.
            color: "#7c93b8",
            totp: false,
            group: "Backend",
            fields: vec![
                F::new("Database URL", true).danger().ty(Url),
                F::new("Direct URL", true).ty(Url),
                F::new("Accelerate API Key", true).ty(ApiKey),
                F::new("Project ID", false),
            ],
        },
        Template {
            id: "claude",
            name: "Claude",
            mono: "Cl",
            slug: Some("claude"),
            icon: None,
            color: "#d97757",
            totp: false,
            group: "AI",
            fields: vec![
                F::new("API Key", true).danger().ty(ApiKey),
                F::new("Workspace ID", false),
                F::new("Organization ID", false),
            ],
        },
        Template {
            id: "resend",
            name: "Resend",
            mono: "Re",
            slug: Some("resend"),
            icon: None,
            // Resend's brand hex is black; use a readable light tint on the dark tile.
            color: "#e8edf2",
            totp: false,
            group: "Backend",
            fields: vec![
                F::new("API Key", true).danger().ty(ApiKey),
                F::new("From Email", false).ty(Email),
                F::new("Webhook Signing Secret", true),
            ],
        },
        Template {
            id: "runpod",
            name: "RunPod",
            mono: "RP",
            slug: None,
            icon: Some("layers"),
            color: "#673ab7",
            totp: false,
            group: "Infra",
            fields: vec![
                F::new("API Key", true).danger().ty(ApiKey),
                F::new("Endpoint ID", false),
                F::new("Account Email", false).ty(Email),
            ],
        },
        Template {
            id: "zeroid",
            name: "ZeroID",
            mono: "Z0",
            slug: None,
            icon: Some("shield"),
            color: "#34e29a",
            totp: true,
            group: "Auth",
            fields: vec![
                F::new("Client ID", false),
                F::new("Client Secret", true).danger().ty(ApiKey),
                F::new("Issuer URL", false).ty(Url),
                F::new("Account Email", false).ty(Email),
            ],
        },
    ]
}

/// Look up a template by id.
pub fn find(id: &str) -> Option<Template> {
    catalog().into_iter().find(|t| t.id == id)
}
