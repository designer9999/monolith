//! Temporary localhost bridge for local AI agents.
//!
//! The bridge is intentionally narrow: it exposes a machine-readable import
//! contract and accepts import bundles while the vault is unlocked. It never
//! returns existing vault data or revealed secret values.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::agent_import;
use crate::error::{AppError, AppResult};
use crate::models::{AgentBridgeSession, AgentImportBundle, AgentImportResult};
use crate::pairing;
use crate::state::Inner;
use crate::templates;
use crate::vault::crypto;

const BRIDGE_TTL_SECONDS: i64 = 30 * 60;
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
const READ_TIMEOUT_SECONDS: u64 = 6;

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
    import: String,
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
    expires_at: String,
}

pub fn start(
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
                let inner = Arc::clone(&inner);
                let runtime = runtime.clone();
                thread::spawn(move || {
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
                "capabilities": "/agent/capabilities"
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
        ("POST", "/agent/import") => {
            let bundle: AgentImportBundle =
                serde_json::from_slice(&request.body).map_err(|_| {
                    AppError::Invalid("request body must be a MONOLITH import JSON bundle".into())
                })?;
            let result = import_bundle(inner, &bundle)?;
            write_json(stream, 200, &result)?;
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
    let query_token = request
        .path
        .split_once('?')
        .and_then(|(_, query)| parse_query(query).get("token").cloned());
    header_token
        .map(str::trim)
        .or(query_token.as_deref())
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
            import: session.import_url.clone(),
        },
        authentication: AgentBridgeAuth {
            scheme: "token",
            header: "X-MONOLITH-Agent-Token",
            note: "Pass the temporary token from Settings. This bridge listens only on 127.0.0.1 and expires automatically.",
        },
        limits: AgentBridgeLimits {
            max_body_bytes: MAX_BODY_BYTES,
            max_items: 500,
            expires_at: session.expires_at.clone(),
        },
        import_schema: import_schema(),
        supported_template_ids,
        templates,
        example_bundle: json!({
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
        }),
        agent_prompt: agent_prompt(session),
    })
}

fn import_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "required": ["items"],
        "properties": {
            "version": { "const": 1 },
            "source": { "type": "string" },
            "defaultProjectId": { "type": "string" },
            "defaultProjectName": { "type": "string" },
            "items": {
                "type": "array",
                "minItems": 1,
                "maxItems": 500,
                "items": {
                    "type": "object",
                    "required": ["templateId"],
                    "properties": {
                        "projectId": { "type": "string" },
                        "projectName": { "type": "string" },
                        "templateId": { "type": "string" },
                        "label": { "type": "string" },
                        "env": { "enum": ["production", "staging", "dev", "all"] },
                        "expiresAt": { "type": "string", "pattern": "^\\d{4}-\\d{2}-\\d{2}$" },
                        "totpSecret": { "type": "string" },
                        "source": { "type": "string" },
                        "fields": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "required": ["label", "value"],
                                "properties": {
                                    "label": { "type": "string" },
                                    "value": { "type": "string" }
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

fn agent_prompt(session: &AgentBridgeSession) -> String {
    format!(
        r#"You are importing credentials into MONOLITH through its local agent bridge.

Security rules:
- Do not print, summarize, or expose secret values in chat.
- Read only the credential paths the user explicitly allows.
- First request MONOLITH capabilities so you know all accepted templates and exact field labels.
- Use stable labels because MONOLITH upserts by project + templateId + label.
- Use defaultProjectName "Personal" for global/personal accounts.
- Use expiresAt only when a real expiration, renewal, or rotation date exists in YYYY-MM-DD format.
- Import with POST /agent/import. Delete any plaintext temporary bundle after success.

Capabilities:
GET {capabilities_url}
Header: X-MONOLITH-Agent-Token: {token}

Import:
POST {import_url}
Header: X-MONOLITH-Agent-Token: {token}
Content-Type: application/json

Allowed bundle root shape:
{{"version":1,"source":"...","defaultProjectName":"Personal","items":[...]}}
"#,
        capabilities_url = session.capabilities_url,
        import_url = session.import_url,
        token = session.token,
    )
}

fn session_from_runtime(runtime: &AgentBridgeRuntime) -> AppResult<AgentBridgeSession> {
    let base_url = format!("http://127.0.0.1:{}", runtime.port);
    Ok(AgentBridgeSession {
        capabilities_url: format!("{base_url}/agent/capabilities"),
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

fn path_without_query(path: &str) -> String {
    path.split_once('?')
        .map(|(path, _)| path)
        .unwrap_or(path)
        .to_string()
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            Some((
                key.to_string(),
                urlencoding::decode(value).ok()?.into_owned(),
            ))
        })
        .collect()
}

fn write_json<T: Serialize>(stream: &mut TcpStream, status: u16, body: &T) -> AppResult<()> {
    let body = serde_json::to_string(body)?;
    write_response(stream, status, "application/json", &body)
}

fn write_error(stream: &mut TcpStream, status: u16, message: &str) -> AppResult<()> {
    write_json(stream, status, &json!({ "error": message }))
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
        410 => "Gone",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {label}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: null\r\nAccess-Control-Allow-Headers: Content-Type, X-MONOLITH-Agent-Token, Authorization\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| AppError::Other(format!("agent bridge response write failed: {e}")))
}
