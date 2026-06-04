//! Temporary localhost bridge for local AI agents.
//!
//! The bridge is intentionally narrow: it exposes a machine-readable import
//! contract and accepts import bundles while the vault is unlocked. It never
//! returns existing vault data or revealed secret values.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::agent_import;
use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::models::{AgentBridgeSession, AgentImportBundle, AgentImportResult};
use crate::pairing;
use crate::state::Inner;
use crate::templates;
use crate::vault::crypto;

const BRIDGE_TTL_SECONDS: i64 = 30 * 60;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_ACTIVE_CONNECTIONS: usize = 8;
const READ_TIMEOUT_SECONDS: u64 = 6;
const IMPORT_SCHEMA: &str = include_str!("../../docs/agent-import.schema.json");
pub const AGENT_IMPORTED_EVENT: &str = "monolith://agent-imported";

#[derive(Default)]
pub struct AgentBridgeStore {
    session: Mutex<Option<AgentBridgeRuntime>>,
}

#[derive(Clone)]
struct AgentBridgeRuntime {
    port: u16,
    token: String,
    expires_at: OffsetDateTime,
    stop: Arc<AtomicBool>,
    active_connections: Arc<AtomicUsize>,
    app: AppHandle,
}

struct ActiveConnection {
    count: Arc<AtomicUsize>,
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentBridgeCapabilities {
    name: &'static str,
    version: u8,
    endpoints: AgentBridgeEndpoints,
    authentication: AgentBridgeAuth,
    limits: AgentBridgeLimits,
    import_schema: serde_json::Value,
    supported_template_ids: Vec<&'static str>,
    templates: Vec<crate::templates::Template>,
    example_bundle: serde_json::Value,
    agent_prompt: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentBridgeEndpoints {
    health: String,
    capabilities: String,
    projects: String,
    import: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentBridgeProject {
    id: String,
    name: String,
    sub: String,
    personal: bool,
    service_count: i64,
    totp_count: i64,
    updated: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentBridgeAuth {
    scheme: &'static str,
    header: &'static str,
    note: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentBridgeLimits {
    max_body_bytes: usize,
    max_items: usize,
    max_active_connections: usize,
    expires_at: String,
}

pub fn start(
    app: AppHandle,
    store: Arc<AgentBridgeStore>,
    inner: Arc<Mutex<Inner>>,
) -> AppResult<AgentBridgeSession> {
    {
        let guard = lock_store(&store)?;
        if let Some(runtime) = guard.as_ref().filter(|runtime| runtime.is_active()) {
            return session_from_runtime(runtime);
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AppError::Other(format!("could not start local agent bridge: {e}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| AppError::Other(format!("could not configure local agent bridge: {e}")))?;

    let token = pairing::encode(&crypto::random_key()?);
    let expires_at = OffsetDateTime::now_utc() + time::Duration::seconds(BRIDGE_TTL_SECONDS);
    let runtime = AgentBridgeRuntime {
        port: listener
            .local_addr()
            .map_err(|e| AppError::Other(format!("could not inspect bridge address: {e}")))?
            .port(),
        token,
        expires_at,
        stop: Arc::new(AtomicBool::new(false)),
        active_connections: Arc::new(AtomicUsize::new(0)),
        app,
    };
    let session = session_from_runtime(&runtime)?;

    {
        let mut guard = lock_store(&store)?;
        if let Some(existing) = guard.replace(runtime.clone()) {
            existing.stop.store(true, Ordering::Relaxed);
        }
    }

    thread::spawn(move || serve_bridge(listener, store, inner, runtime));
    Ok(session)
}

pub fn stop(store: &AgentBridgeStore) -> AppResult<()> {
    let mut guard = store
        .session
        .lock()
        .map_err(|_| AppError::Other("agent bridge mutex poisoned".into()))?;
    if let Some(runtime) = guard.take() {
        runtime.stop.store(true, Ordering::Relaxed);
    }
    Ok(())
}

pub fn status(store: &AgentBridgeStore) -> AppResult<Option<AgentBridgeSession>> {
    let mut guard = store
        .session
        .lock()
        .map_err(|_| AppError::Other("agent bridge mutex poisoned".into()))?;
    if let Some(runtime) = guard.as_ref() {
        if runtime.is_active() {
            return session_from_runtime(runtime).map(Some);
        }
    }
    if let Some(runtime) = guard.take() {
        runtime.stop.store(true, Ordering::Relaxed);
    }
    Ok(None)
}

fn lock_store(
    store: &AgentBridgeStore,
) -> AppResult<std::sync::MutexGuard<'_, Option<AgentBridgeRuntime>>> {
    store
        .session
        .lock()
        .map_err(|_| AppError::Other("agent bridge mutex poisoned".into()))
}

fn serve_bridge(
    listener: TcpListener,
    store: Arc<AgentBridgeStore>,
    inner: Arc<Mutex<Inner>>,
    runtime: AgentBridgeRuntime,
) {
    while runtime.is_active() {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let allowed = runtime
                    .active_connections
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                        (active < MAX_ACTIVE_CONNECTIONS).then_some(active + 1)
                    })
                    .is_ok();
                if !allowed {
                    let _ = write_error(&mut stream, 429, "agent bridge is busy");
                    continue;
                }
                let inner = Arc::clone(&inner);
                let runtime = runtime.clone();
                thread::spawn(move || {
                    let _active = ActiveConnection {
                        count: Arc::clone(&runtime.active_connections),
                    };
                    let _ = handle_request(&mut stream, &inner, &runtime);
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(75));
            }
            Err(_) => break,
        }
    }

    if let Ok(mut guard) = store.session.lock() {
        if guard
            .as_ref()
            .is_some_and(|active| active.token == runtime.token)
        {
            *guard = None;
        }
    }
}

fn handle_request(
    stream: &mut TcpStream,
    inner: &Arc<Mutex<Inner>>,
    runtime: &AgentBridgeRuntime,
) -> AppResult<()> {
    stream
        .set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECONDS)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECONDS)))
        .ok();

    let request = match read_request(stream) {
        Ok(request) => request,
        Err(err) => {
            write_error(stream, 400, &err.to_string())?;
            return Ok(());
        }
    };
    let path = path_without_query(&request.path);

    if request.method == "OPTIONS" {
        write_empty(stream, 204)?;
        return Ok(());
    }
    if request.method == "GET" && path == "/agent/health" {
        write_json(
            stream,
            200,
            &json!({
                "ok": true,
                "name": "MONOLITH Agent Bridge",
                "authRequired": true,
                "capabilities": "/agent/capabilities",
                "projects": "/agent/projects",
                "import": "/agent/import"
            }),
        )?;
        return Ok(());
    }

    if !is_authorized(&request, runtime) {
        write_error(stream, 401, "missing or invalid local agent bridge token")?;
        return Ok(());
    }
    if !runtime.is_active() {
        write_error(stream, 410, "local agent bridge session expired")?;
        return Ok(());
    }

    match (request.method.as_str(), path.as_str()) {
        ("GET", "/agent/capabilities") => {
            let session = session_from_runtime(runtime)?;
            write_json(stream, 200, &capabilities(&session)?)?;
        }
        ("GET", "/agent/projects") => {
            write_json(stream, 200, &list_agent_projects(inner)?)?;
        }
        ("POST", "/agent/import") => {
            let result = serde_json::from_slice::<AgentImportBundle>(&request.body)
                .map_err(|_| {
                    AppError::Invalid("request body must be a MONOLITH import JSON bundle".into())
                })
                .and_then(|bundle| import_bundle(inner, &bundle));
            match result {
                Ok(result) => {
                    if result.created + result.updated > 0 {
                        runtime.app.emit(AGENT_IMPORTED_EVENT, &result).ok();
                    }
                    write_json(stream, 200, &result)?;
                }
                Err(err) => write_app_error(stream, &err)?,
            }
        }
        _ => write_error(stream, 404, "agent bridge endpoint not found")?,
    }
    Ok(())
}

fn import_bundle(
    inner: &Arc<Mutex<Inner>>,
    bundle: &AgentImportBundle,
) -> AppResult<AgentImportResult> {
    let guard = inner
        .lock()
        .map_err(|_| AppError::Other("state mutex poisoned".into()))?;
    let key = guard.key.as_ref().ok_or(AppError::Locked)?;
    agent_import::import_bundle(&guard.conn, key, bundle)
}

fn list_agent_projects(inner: &Arc<Mutex<Inner>>) -> AppResult<Vec<AgentBridgeProject>> {
    let guard = inner
        .lock()
        .map_err(|_| AppError::Other("state mutex poisoned".into()))?;
    if guard.key.is_none() {
        return Err(AppError::Locked);
    }
    Ok(repo::list_projects(&guard.conn)?
        .into_iter()
        .map(|project| AgentBridgeProject {
            id: project.id,
            name: project.name,
            sub: project.sub,
            personal: project.personal,
            service_count: project.count,
            totp_count: project.totp_count,
            updated: project.updated,
        })
        .collect())
}

fn read_request(stream: &mut TcpStream) -> AppResult<HttpRequest> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 4096];
    let header_end;
    loop {
        let n = stream
            .read(&mut buf)
            .map_err(|e| AppError::Other(format!("agent bridge request read failed: {e}")))?;
        if n == 0 {
            return Err(AppError::Invalid("empty request".into()));
        }
        bytes.extend_from_slice(&buf[..n]);
        if bytes.len() > MAX_HEADER_BYTES + MAX_BODY_BYTES {
            return Err(AppError::Invalid("request is too large".into()));
        }
        if let Some(pos) = find_header_end(&bytes) {
            header_end = pos;
            break;
        }
        if bytes.len() > MAX_HEADER_BYTES {
            return Err(AppError::Invalid("request headers are too large".into()));
        }
    }

    let header_text = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| AppError::Invalid("request headers must be UTF-8".into()))?;
    let mut lines = header_text.lines();
    let first_line = lines
        .next()
        .ok_or_else(|| AppError::Invalid("missing request line".into()))?;
    let mut first = first_line.split_whitespace();
    let method = first.next().unwrap_or_default().to_string();
    let path = first.next().unwrap_or_default().to_string();
    if method.is_empty() || path.is_empty() {
        return Err(AppError::Invalid("malformed request line".into()));
    }

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length = headers
        .get("content-length")
        .map(|v| v.parse::<usize>())
        .transpose()
        .map_err(|_| AppError::Invalid("content-length must be numeric".into()))?
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err(AppError::Invalid("request body is too large".into()));
    }

    let body_start = header_end + 4;
    while bytes.len() < body_start + content_length {
        let n = stream
            .read(&mut buf)
            .map_err(|e| AppError::Other(format!("agent bridge request body failed: {e}")))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    if bytes.len() < body_start + content_length {
        return Err(AppError::Invalid("request body ended early".into()));
    }

    Ok(HttpRequest {
        method,
        path,
        headers,
        body: bytes[body_start..body_start + content_length].to_vec(),
    })
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn is_authorized(request: &HttpRequest, runtime: &AgentBridgeRuntime) -> bool {
    let header_token = request
        .headers
        .get("x-monolith-agent-token")
        .map(String::as_str)
        .or_else(|| {
            request.headers.get("authorization").and_then(|value| {
                value
                    .strip_prefix("Bearer ")
                    .or_else(|| value.strip_prefix("bearer "))
            })
        });
    header_token
        .map(str::trim)
        .is_some_and(|token| token == runtime.token)
}

fn capabilities(session: &AgentBridgeSession) -> AppResult<AgentBridgeCapabilities> {
    let templates = templates::catalog();
    let supported_template_ids = templates.iter().map(|template| template.id).collect();
    Ok(AgentBridgeCapabilities {
        name: "MONOLITH Agent Bridge",
        version: 1,
        endpoints: AgentBridgeEndpoints {
            health: format!("{}/agent/health", session.base_url),
            capabilities: session.capabilities_url.clone(),
            projects: session.projects_url.clone(),
            import: session.import_url.clone(),
        },
        authentication: AgentBridgeAuth {
            scheme: "token",
            header: "X-MONOLITH-Agent-Token",
            note: "Pass the temporary token from Settings. This bridge listens only on 127.0.0.1 and expires automatically.",
        },
        limits: AgentBridgeLimits {
            max_body_bytes: MAX_BODY_BYTES,
            max_items: agent_import::MAX_AGENT_IMPORT_ITEMS,
            max_active_connections: MAX_ACTIVE_CONNECTIONS,
            expires_at: session.expires_at.clone(),
        },
        import_schema: import_schema()?,
        supported_template_ids,
        templates,
        example_bundle: json!({
            "version": 1,
            "source": "local credential folders",
            "defaultProjectId": "p_personal",
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
        }),
        agent_prompt: agent_prompt(session, false),
    })
}

fn import_schema() -> AppResult<serde_json::Value> {
    let mut schema: serde_json::Value = serde_json::from_str(IMPORT_SCHEMA)?;
    if let Some(items) = schema
        .pointer_mut("/properties/items")
        .and_then(serde_json::Value::as_object_mut)
    {
        items.insert(
            "maxItems".to_string(),
            serde_json::Value::from(agent_import::MAX_AGENT_IMPORT_ITEMS),
        );
    }
    if let Some(template_id) = schema
        .pointer_mut("/$defs/item/properties/templateId")
        .and_then(serde_json::Value::as_object_mut)
    {
        template_id.insert(
            "enum".to_string(),
            json!(templates::catalog()
                .iter()
                .map(|template| template.id)
                .collect::<Vec<_>>()),
        );
    }
    Ok(schema)
}

fn agent_prompt(session: &AgentBridgeSession, include_token: bool) -> String {
    let token_line = if include_token {
        session.token.as_str()
    } else {
        "<token from MONOLITH Settings>"
    };
    format!(
        r#"You are importing credentials into MONOLITH through its local agent bridge.

Security rules:
- Do not print, summarize, or expose secret values in chat.
- Read only the credential paths the user explicitly allows.
- First request MONOLITH capabilities so you know all accepted templates and exact field labels.
- Then list MONOLITH projects and ask the user which target project should receive the credentials unless the user already gave a clear target.
- Use stable labels because MONOLITH upserts by project + templateId + label.
- Prefer projectId/defaultProjectId from the project list. Use Personal only when the user explicitly wants global or personal credentials.
- Do not create a new project unless the user explicitly asks for one. If no listed project matches, ask before importing.
- Use expiresAt only when a real expiration, renewal, or rotation date exists in YYYY-MM-DD format.
- Import with POST /agent/import. Delete any plaintext temporary bundle after success.

Capabilities:
GET {capabilities_url}
Header: X-MONOLITH-Agent-Token: {token}

Projects:
GET {projects_url}
Header: X-MONOLITH-Agent-Token: {token}

Import:
POST {import_url}
Header: X-MONOLITH-Agent-Token: {token}
Content-Type: application/json

Allowed bundle root shape:
{{"version":1,"source":"...","defaultProjectId":"<selected project id>","items":[...]}}
"#,
        capabilities_url = session.capabilities_url,
        projects_url = session.projects_url,
        import_url = session.import_url,
        token = token_line,
    )
}

fn session_from_runtime(runtime: &AgentBridgeRuntime) -> AppResult<AgentBridgeSession> {
    let base_url = format!("http://127.0.0.1:{}", runtime.port);
    Ok(AgentBridgeSession {
        capabilities_url: format!("{base_url}/agent/capabilities"),
        projects_url: format!("{base_url}/agent/projects"),
        import_url: format!("{base_url}/agent/import"),
        base_url,
        token: runtime.token.clone(),
        expires_at: runtime
            .expires_at
            .format(&Rfc3339)
            .map_err(|e| AppError::Other(format!("could not format bridge expiry: {e}")))?,
    })
}

impl AgentBridgeRuntime {
    fn is_active(&self) -> bool {
        !self.stop.load(Ordering::Relaxed) && OffsetDateTime::now_utc() < self.expires_at
    }
}

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::AcqRel);
    }
}

fn path_without_query(path: &str) -> String {
    path.split_once('?')
        .map(|(path, _)| path)
        .unwrap_or(path)
        .to_string()
}

fn write_json<T: Serialize>(stream: &mut TcpStream, status: u16, body: &T) -> AppResult<()> {
    let body = serde_json::to_string(body)?;
    write_response(stream, status, "application/json", &body)
}

fn write_error(stream: &mut TcpStream, status: u16, message: &str) -> AppResult<()> {
    write_json(stream, status, &json!({ "error": message }))
}

fn write_app_error(stream: &mut TcpStream, err: &AppError) -> AppResult<()> {
    let status = match err {
        AppError::Locked => 423,
        AppError::BadPassword => 401,
        AppError::VaultState(_) => 409,
        AppError::NotFound(_) => 404,
        AppError::Invalid(_) => 400,
        AppError::Crypto(_) | AppError::Db(_) | AppError::Other(_) => 500,
    };
    write_json(stream, status, &json!({ "error": err.to_string() }))
}

fn write_empty(stream: &mut TcpStream, status: u16) -> AppResult<()> {
    write_response(stream, status, "application/json", "")
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> AppResult<()> {
    let label = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        410 => "Gone",
        423 => "Locked",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {label}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| AppError::Other(format!("agent bridge response write failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> AgentBridgeSession {
        AgentBridgeSession {
            base_url: "http://127.0.0.1:49152".to_string(),
            capabilities_url: "http://127.0.0.1:49152/agent/capabilities".to_string(),
            projects_url: "http://127.0.0.1:49152/agent/projects".to_string(),
            import_url: "http://127.0.0.1:49152/agent/import".to_string(),
            token: "test-token".to_string(),
            expires_at: "2026-06-03T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn import_schema_uses_runtime_limits_and_template_catalog() {
        let schema = import_schema().unwrap();
        let max_items = schema
            .pointer("/properties/items/maxItems")
            .and_then(serde_json::Value::as_u64);
        assert_eq!(max_items, Some(agent_import::MAX_AGENT_IMPORT_ITEMS as u64));

        let enum_values = schema
            .pointer("/$defs/item/properties/templateId/enum")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        let template_ids = templates::catalog()
            .into_iter()
            .map(|template| serde_json::Value::String(template.id.to_string()))
            .collect::<Vec<_>>();
        assert_eq!(enum_values, &template_ids);
    }

    #[test]
    fn capabilities_exposes_import_endpoint_and_write_only_prompt() {
        let caps = capabilities(&test_session()).unwrap();

        assert_eq!(caps.endpoints.import, "http://127.0.0.1:49152/agent/import");
        assert_eq!(
            caps.endpoints.projects,
            "http://127.0.0.1:49152/agent/projects"
        );
        assert_eq!(caps.authentication.header, "X-MONOLITH-Agent-Token");
        assert!(caps
            .agent_prompt
            .contains("Do not print, summarize, or expose secret values"));
        assert!(caps.agent_prompt.contains("/agent/projects"));
        assert!(!caps.agent_prompt.contains("test-token"));
    }
}
