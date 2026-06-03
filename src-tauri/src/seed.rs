//! First-run demo data.
//!
//! Seeds the same projects/services the MONOLITH mockup shows, so a freshly
//! created vault looks alive instead of empty. Every secret here is sealed with
//! the real vault key on insert — this is genuine encrypted data, just synthetic.
//! Runs only when the vault is brand-new (no projects yet).

use rusqlite::Connection;

use crate::error::AppResult;
use crate::models::{AddServiceInput, CreateProjectInput, Environment, ServiceFieldInput};
use crate::vault::VaultKey;

/// A compact description of a seed service: template, label, env, (label,value)
/// pairs, and an optional base32 TOTP secret.
struct SeedService {
    template: &'static str,
    label: &'static str,
    env: Environment,
    totp: Option<&'static str>,
    fields: &'static [(&'static str, &'static str)],
}

struct SeedProject {
    name: &'static str,
    sub: &'static str,
    color: &'static str,
    services: &'static [SeedService],
}

/// Insert the demo content if the vault has no projects yet.
pub fn seed_if_empty(conn: &Connection, key: &VaultKey) -> AppResult<()> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))?;
    if n > 0 {
        return Ok(());
    }

    // Insert in reverse so the first listed project ends up on top (create_project
    // pushes existing projects down).
    for sp in PROJECTS.iter().rev() {
        let pid = crate::db::repo::create_project(
            conn,
            &CreateProjectInput {
                name: sp.name.to_string(),
                sub: sp.sub.to_string(),
                color: sp.color.to_string(),
            },
        )?;
        for ss in sp.services {
            let fields = ss
                .fields
                .iter()
                .map(|(l, v)| ServiceFieldInput {
                    label: l.to_string(),
                    value: v.to_string(),
                })
                .collect();
            crate::db::repo::add_service(
                conn,
                key,
                &AddServiceInput {
                    project_id: pid.clone(),
                    template_id: ss.template.to_string(),
                    label: ss.label.to_string(),
                    env: ss.env,
                    expires_at: None,
                    fields,
                    totp_secret: ss.totp.map(|s| s.to_string()),
                },
            )?;
        }
    }

    // A little recent-activity history for the home screen.
    crate::db::repo::log_activity(conn, "IMPORT", "Vault · demo content", "add")?;
    Ok(())
}

use Environment::{All, Production};

static PROJECTS: &[SeedProject] = &[
    SeedProject {
        name: "Nimbus",
        sub: "SaaS platform",
        color: "#5b9dff",
        services: &[
            SeedService {
                template: "supabase",
                label: "Production",
                env: Production,
                totp: None,
                fields: &[
                    ("Project URL", "https://qkzfx.supabase.co"),
                    (
                        "Anon Key",
                        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJyb2xlIjoiYW5vbiJ9.7xQp",
                    ),
                    ("Service Role Key", "eyJhbGciOiJIUzI1Ni..svc..9aF2RkLp"),
                    ("JWT Secret", "sB9-jwt-N1mbus-secret-2026"),
                    ("Database Password", "pg_N1mb2s_pr0d_88x!"),
                    ("S3 Access Key", "AKIAQKZFX7700NIMBUS"),
                    ("S3 Secret Key", "wJalrXUtn+EXAMPLE/K7MDENG/bPxRfi"),
                ],
            },
            SeedService {
                template: "vercel",
                label: "",
                env: Production,
                totp: None,
                fields: &[
                    ("Account Email", "you@nimbus.app"),
                    ("Access Token", "vercel_pat_9KxQ2mNr8sT4vWb1"),
                    ("Team ID", "team_nimbus"),
                    ("Project ID", "prj_8812ax"),
                    ("Deploy Hook URL", "https://api.vercel.com/v1/hooks/9kx"),
                ],
            },
            SeedService {
                template: "github",
                label: "nimbus-web",
                env: All,
                totp: Some("JBSWY3DPEHPK3PXP"),
                fields: &[
                    ("Username", "you"),
                    ("Personal Access Token", "github_pat_11AABBCCDD_xY9"),
                    (
                        "SSH Private Key",
                        "-----BEGIN OPENSSH PRIVATE KEY-----\nb3Bl..nimbus..key\n-----END-----",
                    ),
                    ("Webhook Secret", "whk_9a8b"),
                    ("OAuth Client ID", "Iv1.8a61f9b3"),
                    ("OAuth Secret", "a1b2c3d4e5f60718"),
                ],
            },
            SeedService {
                template: "postgres",
                label: "Primary",
                env: Production,
                totp: None,
                fields: &[
                    ("Host", "db.qkzfx.supabase.co"),
                    ("Port", "5432"),
                    ("User", "postgres"),
                    ("Password", "pg_N1mb2s_pr0d_88x!"),
                    ("Database", "postgres"),
                    (
                        "Connection URL",
                        "postgresql://postgres:***@db.qkzfx.supabase.co:5432/postgres",
                    ),
                ],
            },
            SeedService {
                template: "openai",
                label: "",
                env: All,
                totp: None,
                fields: &[
                    ("API Key", "sk-proj-N1mbus-aA1bB2cC3"),
                    ("Organization ID", "org-9Kd2"),
                    ("Project ID", "proj_nimbus"),
                ],
            },
        ],
    },
    SeedProject {
        name: "Acme Store",
        sub: "Client · e-commerce",
        color: "#ff8a3d",
        services: &[
            SeedService {
                template: "shopify",
                label: "",
                env: Production,
                totp: Some("KRSXG5CTMVRXEZLU"),
                fields: &[
                    ("Store URL", "acme-store.myshopify.com"),
                    ("Admin Email", "ops@acme.co"),
                    ("Password", "Acme!Retail#2026"),
                    ("Admin API Token", "shpat_9921ac77"),
                ],
            },
            SeedService {
                template: "stripe",
                label: "Live",
                env: Production,
                totp: None,
                fields: &[
                    ("Publishable Key", "pk_live_51HxQz"),
                    ("Secret Key", "sk_live_51HxQz..Lp"),
                    ("Restricted Key", "rk_live_51Hx...Qz"),
                    ("Webhook Secret", "whsec_9a8b7c"),
                    ("Mode", "live"),
                ],
            },
            SeedService {
                template: "cloudflare",
                label: "acme.co",
                env: Production,
                totp: None,
                fields: &[
                    ("Account Email", "ops@acme.co"),
                    ("API Token", "cf_2Ykq9Lp"),
                    ("Global API Key", "globalkey123456acme"),
                    ("Zone ID", "a1b2c3"),
                    ("Account ID", "9f8e7d"),
                ],
            },
        ],
    },
    SeedProject {
        name: "Personal",
        sub: "Accounts & finance",
        color: "#c8ff2e",
        services: &[
            SeedService {
                template: "google",
                label: "Primary",
                env: All,
                totp: Some("GEZDGNBVGY3TQOJQ"),
                fields: &[
                    ("Client ID", "4711.apps.googleusercontent.com"),
                    ("Client Secret", "GOCSPX-aA1bB2"),
                    ("API Key", "AIzaSyB-personal"),
                    (
                        "Service Account JSON",
                        "{ \"type\":\"service_account\", \"project_id\":\"me-personal\" }",
                    ),
                    ("Account Email", "me@gmail.com"),
                    ("Account Password", "sunflower2019"),
                ],
            },
            SeedService {
                template: "login",
                label: "Apple ID",
                env: All,
                totp: Some("MFRGGZDFMZTWQ2LK"),
                fields: &[
                    ("URL", "appleid.apple.com"),
                    ("Email / Username", "me@icloud.com"),
                    ("Password", "Gr@nite-Harbor-71"),
                ],
            },
            SeedService {
                template: "card",
                label: "Visa · Daily",
                env: All,
                totp: None,
                fields: &[
                    ("Card Number", "4921 8841 2200 7741"),
                    ("Expiry", "08 / 29"),
                    ("CVV", "441"),
                    ("Cardholder", "A. KANE"),
                ],
            },
        ],
    },
    SeedProject {
        name: "Infrastructure",
        sub: "Servers & DNS",
        color: "#b98cff",
        services: &[
            SeedService {
                template: "ssh",
                label: "de-fsn1",
                env: Production,
                totp: None,
                fields: &[
                    ("Host", "49.12.x.x"),
                    ("User", "root"),
                    (
                        "Private Key",
                        "-----BEGIN OPENSSH PRIVATE KEY-----\nb3Blbn..fsn1..4421\n-----END-----",
                    ),
                    ("Passphrase", "fsn1-PASS-key-9#"),
                ],
            },
            SeedService {
                template: "cloudflare",
                label: "all zones",
                env: All,
                totp: Some("NB2W45DFOIZA"),
                fields: &[
                    ("Account Email", "ops@nimbus.app"),
                    ("API Token", "cf_infra_88kq"),
                    ("Global API Key", "cfglobal_infra_x"),
                    ("Zone ID", "z-multi"),
                    ("Account ID", "acc-12"),
                ],
            },
            SeedService {
                template: "aws",
                label: "eu-central",
                env: Production,
                totp: Some("ONSWG4TFOQ"),
                fields: &[
                    ("Access Key ID", "AKIAINFRA7700XQ"),
                    ("Secret Access Key", "kS9aZ+infra/secret/9KdP"),
                    ("Region", "eu-central-1"),
                    ("Account ID", "4471-9920"),
                ],
            },
        ],
    },
    SeedProject {
        name: "Archive",
        sub: "Legacy projects",
        color: "#6b7280",
        services: &[
            SeedService {
                template: "login",
                label: "Old Heroku",
                env: Environment::Dev,
                totp: None,
                fields: &[
                    ("URL", "heroku.com"),
                    ("Email / Username", "old@gmail.com"),
                    ("Password", "heroku2018"),
                ],
            },
            SeedService {
                template: "domain",
                label: "acme.co",
                env: All,
                totp: None,
                fields: &[
                    ("Registrar", "Namecheap"),
                    ("Login Email", "old@gmail.com"),
                    ("Password", "oldpass1"),
                    ("EPP / Auth Code", "EPP-7741-XZ"),
                    ("Renewal Date", "2027-03-01"),
                ],
            },
        ],
    },
];
