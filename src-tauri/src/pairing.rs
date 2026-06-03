//! Local QR pairing and encrypted one-time vault transfer.
//!
//! The desktop side creates a short-lived session and serves a single LAN HTTP
//! request. The phone side scans the QR payload, performs an ephemeral X25519
//! key agreement, and receives an XChaCha20-Poly1305 encrypted vault package.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, Duration as TimeDuration, OffsetDateTime};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::error::{AppError, AppResult};
use crate::models::{PairingSession, PairingSessionStatus};
use crate::vault::crypto;

const PAIRING_TTL_SECONDS: i64 = 120;
const HTTP_WAIT_SECONDS: u64 = 125;
const PAIRING_CONNECT_SECONDS: u64 = 3;
const PAIRING_KIND: &str = "monolith-pair";
const PAIRING_VERSION: u8 = 1;
const BAD_INTERFACE_KEYWORDS: &[&str] = &[
    "tun",
    "tap",
    "wg",
    "vpn",
    "proton",
    "nord",
    "mullvad",
    "wireguard",
    "virtual",
    "hyper-v",
    "vethernet",
    "vmware",
    "virtualbox",
    "wsl",
    "docker",
    "bluetooth",
    "loopback",
];

#[derive(Default)]
pub struct PairingStore {
    sessions: Mutex<HashMap<String, PairingSessionState>>,
    ready: Condvar,
}

#[derive(Clone)]
pub struct PendingDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub public_key: String,
    pub desktop_secret: [u8; crypto::KEY_LEN],
    pub code: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingQrPayload {
    pub kind: String,
    pub version: u8,
    pub session_id: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<String>,
    pub port: u16,
    pub desktop_public_key: String,
    pub code: String,
    pub expires_at: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedPairingEnvelope {
    pub version: u8,
    pub session_id: String,
    pub device_id: String,
    pub nonce: String,
    pub cipher: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingTransferPackage {
    pub vault_id: String,
    pub device_id: String,
    pub device_key: String,
    pub vault_key: String,
    pub db: String,
    pub created_at: String,
}

struct PairingSessionState {
    code: String,
    expires_at: OffsetDateTime,
    desktop_secret: [u8; crypto::KEY_LEN],
    approved: bool,
    cancelled: bool,
    consumed: bool,
    pending_device: Option<PendingDeviceLite>,
    envelope: Option<EncryptedPairingEnvelope>,
}

#[derive(Clone)]
struct PendingDeviceLite {
    id: String,
    name: String,
    platform: String,
    public_key: String,
}

pub fn start_session(store: Arc<PairingStore>) -> AppResult<PairingSession> {
    let listener = TcpListener::bind("0.0.0.0:0")
        .map_err(|e| AppError::Other(format!("could not start pairing listener: {e}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| AppError::Other(format!("could not configure pairing listener: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::Other(format!("could not read pairing listener address: {e}")))?
        .port();
    let hosts = local_lan_hosts()?;
    let host = hosts
        .first()
        .cloned()
        .ok_or_else(|| AppError::Other("could not determine local pairing IP".into()))?;

    let id = format!("pair_{}", uuid::Uuid::new_v4().simple());
    let code = pairing_code()?;
    let desktop_secret = crypto::random_key()?;
    let desktop_public = PublicKey::from(&StaticSecret::from(desktop_secret));
    let desktop_public_key = encode(desktop_public.as_bytes());
    let expires_at = OffsetDateTime::now_utc() + TimeDuration::seconds(PAIRING_TTL_SECONDS);
    let expires_at_label = format_time(expires_at);

    let qr = PairingQrPayload {
        kind: PAIRING_KIND.to_string(),
        version: PAIRING_VERSION,
        session_id: id.clone(),
        host: host.clone(),
        hosts,
        port,
        desktop_public_key,
        code: code.clone(),
        expires_at: expires_at_label.clone(),
    };
    let qr_payload = serde_json::to_string(&qr)?;

    let state = PairingSessionState {
        code: code.clone(),
        expires_at,
        desktop_secret,
        approved: false,
        cancelled: false,
        consumed: false,
        pending_device: None,
        envelope: None,
    };

    {
        let mut sessions = store
            .sessions
            .lock()
            .map_err(|_| AppError::Other("pairing mutex poisoned".into()))?;
        sessions.insert(id.clone(), state);
    }

    let server_store = Arc::clone(&store);
    let server_session_id = id.clone();
    thread::spawn(move || serve_pairing(listener, server_store, server_session_id));

    Ok(PairingSession {
        id,
        qr_payload,
        code,
        host,
        port,
        expires_at: expires_at_label,
        approved: false,
        pending_device_name: None,
    })
}

pub fn session_status(store: &PairingStore, session_id: &str) -> AppResult<PairingSessionStatus> {
    let sessions = store
        .sessions
        .lock()
        .map_err(|_| AppError::Other("pairing mutex poisoned".into()))?;
    let state = sessions
        .get(session_id)
        .ok_or_else(|| AppError::NotFound(format!("pairing session {session_id}")))?;
    Ok(PairingSessionStatus {
        id: session_id.to_string(),
        approved: state.approved,
        expired: is_expired(state),
        pending_device_name: state.pending_device.as_ref().map(|d| d.name.clone()),
    })
}

pub fn cancel_session(store: &PairingStore, session_id: &str) -> AppResult<()> {
    let mut sessions = store
        .sessions
        .lock()
        .map_err(|_| AppError::Other("pairing mutex poisoned".into()))?;
    let state = sessions
        .get_mut(session_id)
        .ok_or_else(|| AppError::NotFound(format!("pairing session {session_id}")))?;
    state.cancelled = true;
    store.ready.notify_all();
    Ok(())
}

pub fn pending_device_for_approval(
    store: &PairingStore,
    session_id: &str,
) -> AppResult<PendingDevice> {
    let sessions = store
        .sessions
        .lock()
        .map_err(|_| AppError::Other("pairing mutex poisoned".into()))?;
    let state = sessions
        .get(session_id)
        .ok_or_else(|| AppError::NotFound(format!("pairing session {session_id}")))?;
    if is_expired(state) {
        return Err(AppError::Invalid("Pairing session expired".into()));
    }
    if state.consumed {
        return Err(AppError::Invalid("Pairing session was already used".into()));
    }
    let pending = state
        .pending_device
        .clone()
        .ok_or_else(|| AppError::Invalid("No phone is waiting for this pairing session".into()))?;
    Ok(PendingDevice {
        id: pending.id,
        name: pending.name,
        platform: pending.platform,
        public_key: pending.public_key,
        desktop_secret: state.desktop_secret,
        code: state.code.clone(),
    })
}

pub fn approve_session(
    store: &PairingStore,
    session_id: &str,
    envelope: EncryptedPairingEnvelope,
) -> AppResult<()> {
    let mut sessions = store
        .sessions
        .lock()
        .map_err(|_| AppError::Other("pairing mutex poisoned".into()))?;
    let state = sessions
        .get_mut(session_id)
        .ok_or_else(|| AppError::NotFound(format!("pairing session {session_id}")))?;
    if is_expired(state) {
        return Err(AppError::Invalid("Pairing session expired".into()));
    }
    state.approved = true;
    state.envelope = Some(envelope);
    store.ready.notify_all();
    Ok(())
}

pub fn parse_qr_payload(raw: &str) -> AppResult<PairingQrPayload> {
    let payload: PairingQrPayload = serde_json::from_str(raw)
        .map_err(|_| AppError::Invalid("Scanned QR is not a MONOLITH pairing payload".into()))?;
    if payload.kind != PAIRING_KIND || payload.version != PAIRING_VERSION {
        return Err(AppError::Invalid("Unsupported pairing QR version".into()));
    }
    Ok(payload)
}

pub fn create_envelope(
    session_id: &str,
    code: &str,
    desktop_secret: [u8; crypto::KEY_LEN],
    phone_public_key: &str,
    device_id: &str,
    package: &PairingTransferPackage,
) -> AppResult<EncryptedPairingEnvelope> {
    let phone_public = public_key_from_b64(phone_public_key)?;
    let shared = StaticSecret::from(desktop_secret).diffie_hellman(&phone_public);
    let key = derive_transport_key(shared.as_bytes(), session_id, code);
    let plaintext = serde_json::to_vec(package)?;
    let aad = pairing_aad(session_id);
    let (nonce, cipher) = crypto::encrypt(&key, &plaintext, &aad)?;
    Ok(EncryptedPairingEnvelope {
        version: PAIRING_VERSION,
        session_id: session_id.to_string(),
        device_id: device_id.to_string(),
        nonce: encode(&nonce),
        cipher: encode(&cipher),
    })
}

pub fn fetch_and_decrypt_pairing(
    qr: &PairingQrPayload,
    device_id: &str,
    device_name: &str,
    platform: &str,
) -> AppResult<PairingTransferPackage> {
    let phone_secret = crypto::random_key()?;
    let phone_public = PublicKey::from(&StaticSecret::from(phone_secret));
    let phone_public_key = encode(phone_public.as_bytes());
    let body = fetch_pairing_envelope(qr, device_id, device_name, platform, &phone_public_key)?;
    let envelope: EncryptedPairingEnvelope = serde_json::from_str(&body)
        .map_err(|_| AppError::Invalid("Pairing response was not valid JSON".into()))?;
    if envelope.session_id != qr.session_id || envelope.device_id != device_id {
        return Err(AppError::Invalid(
            "Pairing response did not match the request".into(),
        ));
    }

    let desktop_public = public_key_from_b64(&qr.desktop_public_key)?;
    let shared = StaticSecret::from(phone_secret).diffie_hellman(&desktop_public);
    let key = derive_transport_key(shared.as_bytes(), &qr.session_id, &qr.code);
    let nonce = decode(&envelope.nonce)?;
    let cipher = decode(&envelope.cipher)?;
    let plain = crypto::decrypt(&key, &nonce, &cipher, &pairing_aad(&qr.session_id))?;
    serde_json::from_slice(&plain)
        .map_err(|_| AppError::Invalid("Pairing package was not valid JSON".into()))
}

pub fn encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn decode(value: &str) -> AppResult<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::Invalid("invalid base64url value".into()))
}

pub fn bytes32_from_b64(value: &str, label: &str) -> AppResult<[u8; crypto::KEY_LEN]> {
    let bytes = decode(value)?;
    bytes
        .try_into()
        .map_err(|_| AppError::Invalid(format!("{label} has wrong length")))
}

fn serve_pairing(listener: TcpListener, store: Arc<PairingStore>, session_id: String) {
    let started = std::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(HTTP_WAIT_SECONDS) {
            let _ = cancel_session(&store, &session_id);
            return;
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = handle_pairing_request(&mut stream, &store, &session_id);
                return;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(_) => {
                let _ = cancel_session(&store, &session_id);
                return;
            }
        }
    }
}

fn handle_pairing_request(
    stream: &mut TcpStream,
    store: &PairingStore,
    expected_session_id: &str,
) -> AppResult<()> {
    let mut buf = [0u8; 4096];
    let n = stream
        .read(&mut buf)
        .map_err(|e| AppError::Other(format!("pairing request read failed: {e}")))?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let Some(first_line) = request.lines().next() else {
        write_response(stream, 400, r#"{"error":"empty request"}"#)?;
        return Ok(());
    };
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method != "GET" || !path.starts_with("/pair?") {
        write_response(stream, 404, r#"{"error":"not found"}"#)?;
        return Ok(());
    }
    let query = parse_query(path.split_once('?').map(|(_, q)| q).unwrap_or_default());
    let session_id = query.get("sessionId").cloned().unwrap_or_default();
    if session_id != expected_session_id {
        write_response(stream, 404, r#"{"error":"unknown session"}"#)?;
        return Ok(());
    }
    let pending = PendingDeviceLite {
        id: query.get("deviceId").cloned().unwrap_or_default(),
        name: query
            .get("deviceName")
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Android phone".to_string()),
        platform: query
            .get("platform")
            .cloned()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "android".to_string()),
        public_key: query.get("phonePublicKey").cloned().unwrap_or_default(),
    };
    if pending.id.is_empty() || pending.public_key.is_empty() {
        write_response(stream, 400, r#"{"error":"missing pairing parameters"}"#)?;
        return Ok(());
    }

    let mut sessions = store
        .sessions
        .lock()
        .map_err(|_| AppError::Other("pairing mutex poisoned".into()))?;
    {
        let state = sessions
            .get_mut(expected_session_id)
            .ok_or_else(|| AppError::NotFound(format!("pairing session {expected_session_id}")))?;
        if is_expired(state) || state.cancelled || state.consumed {
            write_response(stream, 410, r#"{"error":"pairing expired"}"#)?;
            return Ok(());
        }
        state.pending_device = Some(pending);
    }
    store.ready.notify_all();

    let deadline = std::time::Instant::now() + Duration::from_secs(HTTP_WAIT_SECONDS);
    loop {
        let state = sessions
            .get_mut(expected_session_id)
            .ok_or_else(|| AppError::NotFound(format!("pairing session {expected_session_id}")))?;
        if state.cancelled || is_expired(state) {
            write_response(stream, 410, r#"{"error":"pairing expired"}"#)?;
            return Ok(());
        }
        if let Some(envelope) = state.envelope.take() {
            state.consumed = true;
            let body = serde_json::to_string(&envelope)?;
            write_response(stream, 200, &body)?;
            return Ok(());
        }
        let now = std::time::Instant::now();
        if now >= deadline {
            write_response(stream, 408, r#"{"error":"approval timed out"}"#)?;
            return Ok(());
        }
        let timeout = (deadline - now).min(Duration::from_millis(500));
        let (next, _) = store
            .ready
            .wait_timeout(sessions, timeout)
            .map_err(|_| AppError::Other("pairing mutex poisoned".into()))?;
        sessions = next;
    }
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) -> AppResult<()> {
    let label = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        408 => "Request Timeout",
        410 => "Gone",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {label}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| AppError::Other(format!("pairing response write failed: {e}")))
}

fn fetch_pairing_envelope(
    qr: &PairingQrPayload,
    device_id: &str,
    device_name: &str,
    platform: &str,
    phone_public_key: &str,
) -> AppResult<String> {
    let hosts = pairing_host_candidates(qr);
    let mut errors = Vec::new();
    for host in &hosts {
        match fetch_pairing_envelope_from_host(
            qr,
            host,
            device_id,
            device_name,
            platform,
            phone_public_key,
        ) {
            Ok(body) => return Ok(body),
            Err(err) => errors.push(format!("{host}: {err}")),
        }
    }

    Err(AppError::Other(format!(
        "could not connect to desktop pairing server at {}:{} ({})",
        hosts.join(", "),
        qr.port,
        errors.join("; ")
    )))
}

fn fetch_pairing_envelope_from_host(
    qr: &PairingQrPayload,
    host: &str,
    device_id: &str,
    device_name: &str,
    platform: &str,
    phone_public_key: &str,
) -> AppResult<String> {
    let mut stream = connect_pairing_stream(host, qr.port)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(HTTP_WAIT_SECONDS)))
        .ok();
    stream
        .set_write_timeout(Some(Duration::from_secs(PAIRING_CONNECT_SECONDS)))
        .ok();
    let path = format!(
        "/pair?sessionId={}&deviceId={}&deviceName={}&platform={}&phonePublicKey={}",
        enc(&qr.session_id),
        enc(device_id),
        enc(device_name),
        enc(platform),
        enc(phone_public_key)
    );
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
        host, qr.port
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| AppError::Other(format!("pairing request failed: {e}")))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| AppError::Other(format!("pairing response failed: {e}")))?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| AppError::Invalid("Desktop pairing response was malformed".into()))?;
    if !head.starts_with("HTTP/1.1 200") {
        let status = head.lines().next().unwrap_or("HTTP error");
        let detail = body.trim().trim_matches(char::from(0));
        let message = if detail.is_empty() {
            status.to_string()
        } else {
            format!("{status}: {detail}")
        };
        return Err(AppError::Invalid(message));
    }
    Ok(body.to_string())
}

fn connect_pairing_stream(host: &str, port: u16) -> AppResult<TcpStream> {
    let timeout = Duration::from_secs(PAIRING_CONNECT_SECONDS);
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| AppError::Other(format!("could not resolve {host}:{port}: {e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(AppError::Other(format!(
            "could not resolve {host}:{port} to an address"
        )));
    }

    let mut last_error = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                stream.set_nodelay(true).ok();
                return Ok(stream);
            }
            Err(err) => last_error = Some(format!("{addr}: {err}")),
        }
    }

    Err(AppError::Other(format!(
        "connection timed out or failed ({})",
        last_error.unwrap_or_else(|| "no address attempted".to_string())
    )))
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

fn enc(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

fn public_key_from_b64(value: &str) -> AppResult<PublicKey> {
    let bytes = bytes32_from_b64(value, "public key")?;
    Ok(PublicKey::from(bytes))
}

fn pairing_aad(session_id: &str) -> Vec<u8> {
    format!("monolith-pairing|{session_id}").into_bytes()
}

fn derive_transport_key(
    shared: &[u8; crypto::KEY_LEN],
    session_id: &str,
    code: &str,
) -> [u8; crypto::KEY_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(b"MONOLITH local pairing v1");
    hasher.update(shared);
    hasher.update(session_id.as_bytes());
    hasher.update(code.as_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; crypto::KEY_LEN];
    key.copy_from_slice(&digest);
    key
}

fn local_lan_hosts() -> AppResult<Vec<String>> {
    if let Ok(value) = std::env::var("MONOLITH_PAIR_HOST") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            if let Ok(IpAddr::V4(ip)) = trimmed.parse::<IpAddr>() {
                if !is_usable_ipv4(ip) {
                    return Err(AppError::Invalid(format!(
                        "MONOLITH_PAIR_HOST points to an unusable pairing IP: {trimmed}"
                    )));
                }
            }
            return Ok(vec![trimmed.to_string()]);
        }
    }

    let hosts: Vec<String> = local_lan_ipv4_candidates()
        .into_iter()
        .map(|ip| ip.to_string())
        .collect();
    if hosts.is_empty() {
        return Err(AppError::Other(
            "could not determine a usable local LAN IPv4 address for pairing".into(),
        ));
    }
    Ok(hosts)
}

fn local_lan_ipv4_candidates() -> Vec<Ipv4Addr> {
    let interfaces = local_ip_address::list_afinet_netifas().unwrap_or_default();
    let mut scored: Vec<(Ipv4Addr, i32)> = interfaces
        .iter()
        .filter_map(|(name, ip)| match ip {
            IpAddr::V4(v4) => Some((*v4, local_interface_score(name, *v4))),
            _ => None,
        })
        .collect();
    scored.sort_by(|(left_ip, left_score), (right_ip, right_score)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_ip.octets().cmp(&right_ip.octets()))
    });

    let mut hosts = unique_usable_ips(
        scored
            .iter()
            .filter(|(_, score)| *score > 0)
            .map(|(ip, _)| *ip),
    );
    if !hosts.is_empty() {
        return hosts;
    }

    hosts = unique_usable_ips(interfaces.iter().filter_map(|(name, ip)| {
        if is_bad_interface(name) {
            return None;
        }
        match ip {
            IpAddr::V4(v4) if is_usable_ipv4(*v4) => Some(*v4),
            _ => None,
        }
    }));
    if !hosts.is_empty() {
        return hosts;
    }

    unique_usable_ips(interfaces.iter().filter_map(|(_, ip)| match ip {
        IpAddr::V4(v4) if is_usable_ipv4(*v4) => Some(*v4),
        _ => None,
    }))
}

fn unique_usable_ips(ips: impl Iterator<Item = Ipv4Addr>) -> Vec<Ipv4Addr> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for ip in ips {
        if seen.insert(ip) {
            unique.push(ip);
        }
    }
    unique
}

fn pairing_host_candidates(qr: &PairingQrPayload) -> Vec<String> {
    let mut hosts = Vec::with_capacity(qr.hosts.len() + 1);
    hosts.push(qr.host.clone());
    hosts.extend(qr.hosts.iter().cloned());
    order_pairing_hosts(hosts, local_lan_ipv4_candidates().first().copied())
}

fn order_pairing_hosts(hosts: Vec<String>, local_ip: Option<Ipv4Addr>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ordered: Vec<String> = hosts
        .into_iter()
        .map(|host| host.trim().to_string())
        .filter(|host| !host.is_empty() && seen.insert(host.clone()))
        .collect();

    ordered.sort_by_key(|host| match (local_ip, host.parse::<Ipv4Addr>()) {
        (Some(local), Ok(ip)) if is_same_lan_ipv4(local, ip) => 0,
        (_, Ok(ip)) if is_usable_ipv4(ip) => 1,
        (_, Ok(_)) => 3,
        _ => 2,
    });
    ordered
}

fn is_bad_interface(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    BAD_INTERFACE_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword))
}

fn is_usable_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 169 && octets[1] == 254
        || octets[3] == 0
        || octets[3] == 255)
}

fn is_same_lan_ipv4(local_ip: Ipv4Addr, peer_ip: Ipv4Addr) -> bool {
    let local = local_ip.octets();
    let peer = peer_ip.octets();
    local[0] == peer[0] && local[1] == peer[1] && local[2] == peer[2]
}

fn private_ipv4_score(ip: Ipv4Addr) -> i32 {
    let octets = ip.octets();
    if octets[0] == 192 && octets[1] == 168 {
        40
    } else if octets[0] == 10 {
        30
    } else if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        20
    } else {
        1
    }
}

fn local_interface_score(name: &str, ip: Ipv4Addr) -> i32 {
    if !is_usable_ipv4(ip) {
        return -1000;
    }

    let mut score = private_ipv4_score(ip);
    if is_bad_interface(name) {
        score -= 1000;
    }
    score
}

fn pairing_code() -> AppResult<String> {
    let bytes = crypto::random_key()?;
    let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % 1_000_000;
    Ok(format!("{value:06}"))
}

fn format_time(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap_or_default()
}

fn is_expired(state: &PairingSessionState) -> bool {
    OffsetDateTime::now_utc() >= state.expires_at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_envelope_decrypts_with_phone_secret() {
        let desktop_secret = crypto::random_key().unwrap();
        let phone_secret = crypto::random_key().unwrap();
        let phone_public = PublicKey::from(&StaticSecret::from(phone_secret));
        let package = PairingTransferPackage {
            vault_id: "v_test".into(),
            device_id: "dev_test".into(),
            device_key: encode(&crypto::random_key().unwrap()),
            vault_key: encode(&crypto::random_key().unwrap()),
            db: encode(b"sqlite-bytes"),
            created_at: "2026-06-02T00:00:00Z".into(),
        };

        let envelope = create_envelope(
            "pair_test",
            "123456",
            desktop_secret,
            &encode(phone_public.as_bytes()),
            "dev_test",
            &package,
        )
        .unwrap();

        let desktop_public = PublicKey::from(&StaticSecret::from(desktop_secret));
        let shared = StaticSecret::from(phone_secret).diffie_hellman(&desktop_public);
        let key = derive_transport_key(shared.as_bytes(), "pair_test", "123456");
        let nonce = decode(&envelope.nonce).unwrap();
        let cipher = decode(&envelope.cipher).unwrap();
        let plain = crypto::decrypt(&key, &nonce, &cipher, &pairing_aad("pair_test")).unwrap();
        let decoded: PairingTransferPackage = serde_json::from_slice(&plain).unwrap();
        assert_eq!(decoded.vault_id, package.vault_id);
        assert_eq!(decoded.device_id, package.device_id);
        assert_eq!(decoded.db, package.db);
    }

    #[test]
    fn virtual_and_loopback_interfaces_are_not_pairing_hosts() {
        assert!(local_interface_score("vEthernet (WSL)", Ipv4Addr::new(172, 20, 1, 1)) < 0);
        assert!(local_interface_score("Docker Desktop", Ipv4Addr::new(192, 168, 65, 1)) < 0);
        assert!(local_interface_score("Loopback", Ipv4Addr::new(127, 0, 0, 1)) < 0);
        assert!(local_interface_score("Wi-Fi", Ipv4Addr::new(192, 168, 1, 25)) > 0);
    }

    #[test]
    fn pairing_hosts_are_deduped_and_same_lan_first() {
        let hosts = order_pairing_hosts(
            vec![
                "172.20.1.1".into(),
                "192.168.1.44".into(),
                "192.168.1.44".into(),
                "desktop.local".into(),
            ],
            Some(Ipv4Addr::new(192, 168, 1, 25)),
        );

        assert_eq!(hosts.first().map(String::as_str), Some("192.168.1.44"));
        assert_eq!(hosts.len(), 3);
    }

    #[test]
    fn qr_payload_accepts_old_payloads_without_hosts() {
        let raw = r#"{
            "kind":"monolith-pair",
            "version":1,
            "sessionId":"pair_test",
            "host":"192.168.1.44",
            "port":12345,
            "desktopPublicKey":"abc",
            "code":"123456",
            "expiresAt":"2026-06-03T00:00:00Z"
        }"#;

        let parsed = parse_qr_payload(raw).unwrap();
        assert!(parsed.hosts.is_empty());
    }
}
