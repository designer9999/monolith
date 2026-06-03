//! Agent import pipeline shared by the Tauri Settings UI and the local CLI.
//!
//! This keeps the "AI-friendly" import path from splitting into two different
//! behaviors: both paths validate templates, upsert by project/template/label,
//! encrypt with the live vault key, and archive replaced secret values.

use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::models::*;
use crate::templates::{self, Template};
use crate::vault::VaultKey;

const MAX_AGENT_IMPORT_ITEMS: usize = 500;

enum AgentImportAction {
    Created,
    Updated,
    Skipped,
}

fn non_empty(value: Option<&String>) -> Option<&str> {
    value.map(|v| v.trim()).filter(|v| !v.is_empty())
}

fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn derive_import_label(item: &AgentImportItem, template: &Template) -> String {
    let explicit = item.label.trim();
    if !explicit.is_empty() {
        return truncate_chars(explicit, 80);
    }

    const PREFERRED: &[&str] = &[
        "Account Email",
        "Email / Username",
        "Username",
        "Project ID",
        "Store URL",
        "URL",
        "Host",
        "Account ID",
        "Client ID",
    ];
    for label in PREFERRED {
        let Some(tf) = template
            .fields
            .iter()
            .find(|f| f.label == *label && !f.secret)
        else {
            continue;
        };
        if let Some(value) = item
            .fields
            .iter()
            .find(|field| field.label == tf.label)
            .map(|field| field.value.trim())
            .filter(|value| !value.is_empty())
        {
            return truncate_chars(value, 80);
        }
    }

    String::new()
}

fn import_project_id(
    conn: &rusqlite::Connection,
    bundle: &AgentImportBundle,
    item: &AgentImportItem,
) -> AppResult<String> {
    if let Some(project_id) = non_empty(item.project_id.as_ref())
        .or_else(|| non_empty(bundle.default_project_id.as_ref()))
    {
        if project_id == repo::PERSONAL_PROJECT_ID {
            repo::ensure_personal_project(conn)?;
            return Ok(repo::PERSONAL_PROJECT_ID.to_string());
        }
        if repo::project_exists(conn, project_id)? {
            return Ok(project_id.to_string());
        }
        return Err(AppError::NotFound(format!("project {project_id}")));
    }

    if let Some(project_name) = non_empty(item.project_name.as_ref())
        .or_else(|| non_empty(bundle.default_project_name.as_ref()))
    {
        return repo::ensure_project_by_name(conn, project_name);
    }

    repo::ensure_personal_project(conn)?;
    Ok(repo::PERSONAL_PROJECT_ID.to_string())
}

fn import_agent_item(
    conn: &rusqlite::Connection,
    key: &VaultKey,
    bundle: &AgentImportBundle,
    item: &AgentImportItem,
) -> AppResult<AgentImportAction> {
    let template_id = item.template_id.trim();
    if template_id.is_empty() {
        return Err(AppError::Invalid("templateId is required".into()));
    }
    let template = templates::find(template_id)
        .ok_or_else(|| AppError::NotFound(format!("template {template_id}")))?;
    if non_empty(item.source.as_ref()).is_some_and(|source| source.chars().count() > 512) {
        return Err(AppError::Invalid("source is too long".into()));
    }
    let project_id = import_project_id(conn, bundle, item)?;
    let label = derive_import_label(item, &template);
    let has_payload = item
        .fields
        .iter()
        .any(|field| !field.value.trim().is_empty())
        || item
            .totp_secret
            .as_deref()
            .map(str::trim)
            .is_some_and(|secret| !secret.is_empty());
    if !has_payload && label.is_empty() {
        return Ok(AgentImportAction::Skipped);
    }

    if let Some(service_id) =
        repo::find_service_id_by_identity(conn, &project_id, template.id, &label)?
    {
        repo::update_service(
            conn,
            key,
            &UpdateServiceInput {
                service_id,
                label,
                env: item.env,
                expires_at: item.expires_at.clone(),
                fields: item.fields.clone(),
                totp_secret: item.totp_secret.clone(),
            },
        )?;
        Ok(AgentImportAction::Updated)
    } else {
        repo::add_service(
            conn,
            key,
            &AddServiceInput {
                project_id,
                template_id: template.id.to_string(),
                label,
                env: item.env,
                expires_at: item.expires_at.clone(),
                fields: item.fields.clone(),
                totp_secret: item.totp_secret.clone(),
            },
        )?;
        Ok(AgentImportAction::Created)
    }
}

pub fn import_bundle(
    conn: &rusqlite::Connection,
    key: &VaultKey,
    bundle: &AgentImportBundle,
) -> AppResult<AgentImportResult> {
    if bundle.version.unwrap_or(1) != 1 {
        return Err(AppError::Invalid(
            "Agent import bundle version must be 1".into(),
        ));
    }
    if bundle.items.is_empty() {
        return Err(AppError::Invalid("Agent import bundle has no items".into()));
    }
    if bundle.items.len() > MAX_AGENT_IMPORT_ITEMS {
        return Err(AppError::Invalid(format!(
            "Agent import bundle is limited to {MAX_AGENT_IMPORT_ITEMS} items"
        )));
    }

    let mut result = AgentImportResult {
        created: 0,
        updated: 0,
        skipped: 0,
        errors: Vec::new(),
    };
    for (index, item) in bundle.items.iter().enumerate() {
        let template_label = templates::find(item.template_id.trim())
            .map(|t| t.name.to_string())
            .unwrap_or_else(|| "Item".to_string());
        let safe_label = if item.label.trim().is_empty() {
            template_label
        } else {
            truncate_chars(item.label.trim(), 80)
        };
        match import_agent_item(conn, key, bundle, item) {
            Ok(AgentImportAction::Created) => result.created += 1,
            Ok(AgentImportAction::Updated) => result.updated += 1,
            Ok(AgentImportAction::Skipped) => result.skipped += 1,
            Err(err) => result.errors.push(AgentImportError {
                index,
                label: safe_label,
                message: err.to_string(),
            }),
        }
    }
    let imported = result.created + result.updated;
    if imported > 0 {
        let source = non_empty(bundle.source.as_ref()).unwrap_or("agent bundle");
        repo::log_activity(
            conn,
            "IMPORT",
            &format!("{source} · {imported} credentials"),
            "add",
        )
        .ok();
    }
    Ok(result)
}
