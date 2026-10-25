//! SubForge-owned public OAuth client. Tokens never cross the webview boundary.
use crate::{
    engine::{self, Control, Engine},
    types::Provider,
};
use anyhow::{bail, ensure, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use futures_util::StreamExt;
use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const AUTH: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const DYNAMIC: &str = "dynamic_agent_client";
const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";

#[derive(Default)]
pub struct AuthState {
    pub lock: tokio::sync::Mutex<()>,
    pending: Mutex<HashMap<String, Arc<Attempt>>>,
}
struct Attempt {
    control: Control,
    status: Mutex<String>,
}
#[derive(Default, Serialize, Deserialize)]
struct Session {
    client_id: String,
    subject: String,
    email: String,
    access_token: String,
    refresh_token: String,
    id_token: String,
    scopes: String,
    expires_at: u64,
    #[serde(default)]
    welcomed: bool,
}
#[derive(Deserialize)]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    id_token: String,
    #[serde(default)]
    scope: Option<String>,
    token_type: String,
    expires_in: u64,
}
#[derive(Serialize)]
pub struct Status {
    pub status: String,
    pub account: String,
    pub welcome: bool,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("SubForge/", env!("CARGO_PKG_VERSION")))
        .build()?)
}
fn path(engine: &Engine, id: &str) -> PathBuf {
    engine.dir.join("openai").join(format!(
        "{}.bin",
        hex::encode(Sha256::digest(id.as_bytes()))
    ))
}
// DPAPI binds this encrypted file to the current Windows account and machine.
fn protect(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into()?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // Windows owns the returned allocation; copy before releasing it with LocalFree.
    unsafe {
        let ok = if decrypt {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        ensure!(ok != 0, "openai_credential_store_error");
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        LocalFree(output.pbData as *mut _);
        Ok(result)
    }
}
fn load(engine: &Engine, id: &str) -> Result<Option<Session>> {
    let file = path(engine, id);
    if !file.exists() {
        return Ok(None);
    }
    let bytes =
        std::fs::read(file).map_err(|_| anyhow::anyhow!("openai_credential_store_error"))?;
    serde_json::from_slice(&protect(&bytes, true)?)
        .map(Some)
        .map_err(|_| anyhow::anyhow!("openai_credential_store_error"))
}
fn save(engine: &Engine, id: &str, session: &Session) -> Result<()> {
    let file = path(engine, id);
    std::fs::create_dir_all(file.parent().unwrap())?;
    engine::atomic(&file, &protect(&serde_json::to_vec(session)?, false)?)
        .map_err(|_| anyhow::anyhow!("openai_credential_store_error"))
}
fn host_id(engine: &Engine) -> Result<String> {
    let file = engine.dir.join("openai-host-id");
    if let Ok(value) = std::fs::read_to_string(&file) {
        ensure!(
            uuid::Uuid::parse_str(value.trim()).is_ok(),
            "openai_host_id_invalid"
        );
        return Ok(format!("urn:uuid:{}", value.trim()));
    }
    let value = uuid::Uuid::new_v4().to_string();
    engine::atomic(&file, value.as_bytes())?;
    Ok(format!("urn:uuid:{value}"))
}
pub async fn status(engine: &Engine, id: &str) -> Result<Status> {
    let _lock = engine.openai.lock.lock().await;
    let session = load(engine, id)?;
    let state = engine
        .openai
        .pending
        .lock()
        .unwrap()
        .get(id)
        .map(|a| a.status.lock().unwrap().clone())
        .unwrap_or_else(|| {
            if session
                .as_ref()
                .is_some_and(|s| !s.refresh_token.is_empty())
            {
                if session.as_ref().is_some_and(|s| has_plan(&s.scopes)) {
                    "connected"
                } else {
                    "openai_plan_permission_required"
                }
            } else {
                "signed_out"
            }
            .into()
        });
    let welcome = state == "connected" && session.as_ref().is_some_and(|s| !s.welcomed);
    let account = session
        .map(|s| {
            format!(
                "{} · {}",
                s.email,
                s.client_id
                    .chars()
                    .rev()
                    .take(8)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            )
        })
        .unwrap_or_default();
    Ok(Status {
        status: state,
        account,
        welcome,
    })
}
pub async fn acknowledge(engine: &Engine, id: &str) -> Result<()> {
    let _lock = engine.openai.lock.lock().await;
    if let Some(mut session) = load(engine, id)? {
        session.welcomed = true;
        save(engine, id, &session)?;
    }
    Ok(())
}
pub fn cancel(engine: &Engine, id: &str) {
    if let Some(attempt) = engine.openai.pending.lock().unwrap().remove(id) {
        attempt
            .control
            .cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
pub fn cancel_all(engine: &Engine) {
    for (_, attempt) in engine.openai.pending.lock().unwrap().drain() {
        attempt
            .control
            .cancel
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
fn authorize_url(
    session: Option<&Session>,
    host: &str,
    redirect: &str,
    state: &str,
    nonce: &str,
    verifier: &str,
) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(&format!("{AUTH}/api/accounts/authorize"))?;
    let issued = session.filter(|s| !s.client_id.is_empty());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut query = url.query_pairs_mut();
    query.extend_pairs([
        (
            "client_id",
            issued.map(|s| s.client_id.as_str()).unwrap_or(DYNAMIC),
        ),
        ("ext_agent_host_id", host),
        ("response_type", "code"),
        ("redirect_uri", redirect),
        (
            "scope",
            "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
        ),
        ("resource", RESOURCE),
        ("state", state),
        ("nonce", nonce),
        ("code_challenge_method", "S256"),
        ("code_challenge", challenge.as_str()),
    ]);
    if let Some(session) = issued {
        if !session.id_token.is_empty() {
            query.append_pair("id_token_hint", &session.id_token);
        }
        if !session.email.is_empty() {
            query.append_pair("login_hint", &session.email);
        }
    } else {
        query.append_pair("agent_name_hint", "SubForge");
    }
    drop(query);
    Ok(url)
}
pub async fn start(engine: Arc<Engine>, id: String) -> Result<()> {
    ensure!(!id.trim().is_empty() && id.len() <= 200, "invalid_provider");
    let _lock = engine.openai.lock.lock().await;
    cancel(&engine, &id);
    let session = load(&engine, &id)?;
    let host = host_id(&engine)?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| anyhow::anyhow!("openai_callback_unavailable"))?;
    let redirect = format!(
        "http://127.0.0.1:{}/auth/callback",
        listener.local_addr()?.port()
    );
    let state = random();
    let nonce = random();
    let verifier = random();
    let url = authorize_url(
        session.as_ref(),
        &host,
        &redirect,
        &state,
        &nonce,
        &verifier,
    )?;
    let attempt = Arc::new(Attempt {
        control: Control::default(),
        status: Mutex::new("waiting".into()),
    });
    engine
        .openai
        .pending
        .lock()
        .unwrap()
        .insert(id.clone(), attempt.clone());
    // Never return or log the authorization URL: it can contain an ID-token hint.
    if crate::agents::open_browser(url.as_str()).is_err() {
        cancel(&engine, &id);
        bail!("openai_browser_failed");
    }
    drop(_lock);
    tokio::spawn(async move {
        let result = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(300), complete(&engine, &id, listener, session, &redirect, &state, &nonce, &verifier, &attempt.control)) =>
                result.unwrap_or_else(|_| Err(anyhow::anyhow!("openai_signin_timeout"))),
            _ = attempt.control.until_canceled() => Err(anyhow::anyhow!("canceled")),
        };
        *attempt.status.lock().unwrap() = match result {
            Ok(()) => "connected".into(),
            Err(e) => e.to_string(),
        };
    });
    Ok(())
}
fn callback(target: &str, state: &str, expected_client: Option<&str>) -> Result<(String, String)> {
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}"))?;
    ensure!(url.path() == "/auth/callback", "openai_callback_invalid");
    let pairs: Vec<_> = url.query_pairs().collect();
    // Ambiguous security-sensitive query parameters must never be accepted.
    for key in ["state", "code", "client_id", "error"] {
        ensure!(
            pairs.iter().filter(|(k, _)| k == key).count() <= 1,
            "openai_callback_invalid"
        );
    }
    let get = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_ref())
    };
    ensure!(get("state") == Some(state), "openai_state_mismatch");
    ensure!(get("error").is_none(), "openai_consent_denied");
    let client = get("client_id").or(expected_client).unwrap_or("");
    ensure!(
        !client.is_empty() && client != DYNAMIC,
        "openai_registration_incomplete"
    );
    if let Some(expected) = expected_client {
        ensure!(client == expected, "openai_account_mismatch");
    }
    let code = get("code")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("openai_callback_invalid"))?;
    Ok((code.into(), client.into()))
}
async fn complete(
    engine: &Engine,
    id: &str,
    listener: TcpListener,
    previous: Option<Session>,
    redirect: &str,
    state: &str,
    nonce: &str,
    verifier: &str,
    control: &Control,
) -> Result<()> {
    let expected = previous.as_ref().map(|s| s.client_id.as_str());
    let (code, client_id) = loop {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(|_| anyhow::anyhow!("openai_callback_unavailable"))?;
        let read = tokio::time::timeout(Duration::from_secs(3), async {
            let mut bytes = Vec::new();
            while bytes.len() < 8192 {
                let mut buffer = [0; 1024];
                let count = socket.read(&mut buffer).await?;
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..count]);
                if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Ok::<_, std::io::Error>(bytes)
        })
        .await;
        let bytes = match read {
            Ok(Ok(bytes)) if !bytes.is_empty() && bytes.len() < 8192 => bytes,
            _ => continue,
        };
        let request = String::from_utf8_lossy(&bytes);
        let target = request
            .lines()
            .next()
            .and_then(|s| s.strip_prefix("GET "))
            .and_then(|s| s.strip_suffix(" HTTP/1.1"));
        let parsed = target
            .ok_or_else(|| anyhow::anyhow!("openai_callback_invalid"))
            .and_then(|t| callback(t, state, expected));
        let valid = parsed.is_ok();
        let body = if valid {
            "Sign-in received. Return to SubForge to check your connection."
        } else {
            "This sign-in callback was rejected. Return to SubForge."
        };
        let response = format!("HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}", if valid { "200 OK" } else { "400 Bad Request" }, body.len(), body);
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            socket.write_all(response.as_bytes()),
        )
        .await;
        match parsed {
            Ok(value) => break value,
            Err(e)
                if e.to_string() == "openai_state_mismatch"
                    || e.to_string() == "openai_callback_invalid" =>
            {
                continue
            }
            Err(e) => return Err(e),
        }
    };
    let tokens = exchange(&[
        ("grant_type", "authorization_code"),
        ("client_id", &client_id),
        ("code", &code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect),
        ("resource", RESOURCE),
    ])
    .await?;
    let claims = validate_identity(&tokens.id_token, &client_id, Some(nonce)).await?;
    let subject = claims["sub"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("openai_identity_invalid"))?;
    if let Some(previous) = &previous {
        ensure!(previous.subject == subject, "openai_account_mismatch");
    }
    let scopes = tokens.scope.unwrap_or_default();
    ensure!(!tokens.refresh_token.is_empty(), "openai_refresh_missing");
    let session = Session {
        client_id,
        subject: subject.into(),
        email: claims["email"].as_str().unwrap_or(subject).into(),
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        id_token: tokens.id_token,
        scopes,
        expires_at: now().saturating_add(tokens.expires_in),
        welcomed: previous.is_some_and(|s| s.welcomed),
    };
    let _lock = engine.openai.lock.lock().await;
    ensure!(!control.canceled(), "canceled");
    save(engine, id, &session)?;
    ensure!(has_plan(&session.scopes), "openai_plan_permission_required");
    Ok(())
}
fn has_plan(scopes: &str) -> bool {
    [PLAN_SCOPE, "resource.invoke"]
        .iter()
        .all(|required| scopes.split_whitespace().any(|s| s == *required))
}
async fn validate_identity(token: &str, client_id: &str, nonce: Option<&str>) -> Result<Value> {
    let invalid = || anyhow::anyhow!("openai_identity_invalid");
    let header = decode_header(token).map_err(|_| invalid())?;
    ensure!(header.alg == Algorithm::RS256, "openai_identity_invalid");
    let keys: JwkSet = client()?
        .get(format!("{AUTH}/.well-known/jwks.json"))
        .send()
        .await
        .map_err(|_| invalid())?
        .error_for_status()
        .map_err(|_| invalid())?
        .json()
        .await
        .map_err(|_| invalid())?;
    let jwk = keys
        .find(header.kid.as_deref().ok_or_else(invalid)?)
        .ok_or_else(invalid)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&[AUTH]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.leeway = 30;
    let value = decode::<Value>(
        token,
        &DecodingKey::from_jwk(jwk).map_err(|_| invalid())?,
        &validation,
    )
    .map_err(|_| invalid())?
    .claims;
    if let Some(nonce) = nonce {
        ensure!(
            value["nonce"].as_str() == Some(nonce),
            "openai_nonce_mismatch"
        );
    }
    Ok(value)
}
async fn exchange(form: &[(&str, &str)]) -> Result<Tokens> {
    let response = client()?
        .post(format!("{AUTH}/api/accounts/oauth/token"))
        .form(form)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("network_error"))?;
    ensure!(
        response.status().is_success(),
        "openai_reauthentication_required"
    );
    let tokens: Tokens = response
        .json()
        .await
        .map_err(|_| anyhow::anyhow!("openai_token_invalid"))?;
    ensure!(
        tokens.token_type.eq_ignore_ascii_case("bearer")
            && !tokens.access_token.is_empty()
            && tokens.expires_in > 0,
        "openai_token_invalid"
    );
    Ok(tokens)
}
async fn access_token(engine: &Engine, id: &str, rejected: Option<&str>) -> Result<String> {
    // All requests for a session share this lock, including rotating-token renewal.
    let _lock = engine.openai.lock.lock().await;
    let mut session = load(engine, id)?.ok_or_else(|| anyhow::anyhow!("openai_signin_required"))?;
    ensure!(!session.refresh_token.is_empty(), "openai_signin_required");
    if session.expires_at <= now() + 60 || rejected.is_some_and(|old| old == session.access_token) {
        let tokens = exchange(&[
            ("grant_type", "refresh_token"),
            ("client_id", &session.client_id),
            ("refresh_token", &session.refresh_token),
            ("resource", RESOURCE),
        ])
        .await?;
        if let Some(scopes) = tokens.scope {
            session.scopes = scopes;
        }
        ensure!(has_plan(&session.scopes), "openai_plan_permission_required");
        if !tokens.id_token.is_empty() {
            let claims = validate_identity(&tokens.id_token, &session.client_id, None).await?;
            ensure!(
                claims["sub"].as_str() == Some(&session.subject),
                "openai_account_mismatch"
            );
            session.id_token = tokens.id_token;
        }
        session.access_token = tokens.access_token;
        if !tokens.refresh_token.is_empty() {
            session.refresh_token = tokens.refresh_token;
        }
        session.expires_at = now().saturating_add(tokens.expires_in);
        save(engine, id, &session)?;
    }
    ensure!(has_plan(&session.scopes), "openai_plan_permission_required");
    Ok(session.access_token)
}
pub async fn signout(engine: &Engine, id: &str) -> Result<bool> {
    let _lock = engine.openai.lock.lock().await;
    cancel(engine, id);
    let Some(mut session) = load(engine, id)? else {
        return Ok(true);
    };
    let mut revoked = session.refresh_token.is_empty();
    if !revoked {
        // Discover the revocation endpoint and keep bearer credentials on the trusted issuer.
        if let Ok(response) = client()?
            .get(format!("{AUTH}/.well-known/openid-configuration"))
            .send()
            .await
        {
            if let Ok(metadata) = response.json::<Value>().await {
                if let Some(url) = metadata["revocation_endpoint"]
                    .as_str()
                    .filter(|s| s.starts_with("https://auth.openai.com/"))
                {
                    for delay in [0, 1, 2] {
                        if delay > 0 {
                            tokio::time::sleep(Duration::from_secs(delay)).await;
                        }
                        if let Ok(response) = client()?
                            .post(url)
                            .form(&[
                                ("token", session.refresh_token.as_str()),
                                ("token_type_hint", "refresh_token"),
                                ("client_id", &session.client_id),
                            ])
                            .send()
                            .await
                        {
                            if response.status() == 200 {
                                revoked = true;
                                break;
                            }
                            if response.status().is_client_error() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    session.access_token.clear();
    session.refresh_token.clear();
    session.id_token.clear();
    session.expires_at = 0;
    save(engine, id, &session)?;
    Ok(revoked)
}
async fn request(
    engine: &Engine,
    p: &Provider,
    body: Option<&Value>,
    control: &Control,
) -> Result<reqwest::Response> {
    let mut token = access_token(engine, &p.id, None).await?;
    for attempt in 0..2 {
        let c = client()?;
        let req = if let Some(body) = body {
            c.post(format!("{RESOURCE}/responses"))
                .header(reqwest::header::ACCEPT, "text/event-stream")
                .json(body)
        } else {
            c.get(format!("{RESOURCE}/models"))
        };
        let req = req
            .bearer_auth(&token)
            .timeout(Duration::from_secs(p.timeout_secs));
        let response = tokio::select! { response = req.send() => response.map_err(|e| anyhow::anyhow!(if e.is_timeout() { "request_timeout" } else { "network_error" }))?, _ = control.until_canceled() => bail!("canceled") };
        if response.status() == 401 && attempt == 0 {
            token = access_token(engine, &p.id, Some(&token)).await?;
            continue;
        }
        let status = response.status().as_u16();
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60)
            .min(86400);
        ensure!(response.status().is_success(), "http_{status}:{retry}");
        return Ok(response);
    }
    bail!("openai_reauthentication_required")
}
pub async fn models(engine: &Engine, p: &Provider, control: &Control) -> Result<Vec<String>> {
    let response = request(engine, p, None, control).await?;
    let value: Value = tokio::select! { v = response.json() => v.map_err(|_| anyhow::anyhow!("invalid_json"))?, _ = control.until_canceled() => bail!("canceled") };
    catalog(&value)
}
fn catalog(value: &Value) -> Result<Vec<String>> {
    let list = value["models"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid_json"))?;
    Ok(list
        .iter()
        .filter(|m| m["visibility"] == "list")
        .filter_map(|m| m["slug"].as_str())
        .map(String::from)
        .collect())
}
#[derive(Default)]
struct ReplyStream {
    text: String,
    started: bool,
    completed: bool,
}
impl ReplyStream {
    fn feed(&mut self, pending: &mut Vec<u8>) -> Result<()> {
        loop {
            let lf = pending
                .windows(2)
                .position(|w| w == b"\n\n")
                .map(|i| (i, 2));
            let crlf = pending
                .windows(4)
                .position(|w| w == b"\r\n\r\n")
                .map(|i| (i, 4));
            let Some((end, delimiter)) = lf.into_iter().chain(crlf).min_by_key(|x| x.0) else {
                break;
            };
            let block = String::from_utf8(pending.drain(..end + delimiter).collect())
                .map_err(|_| anyhow::anyhow!("invalid_json"))?;
            self.event(&block)?;
            if self.completed {
                break;
            }
        }
        Ok(())
    }
    fn event(&mut self, block: &str) -> Result<()> {
        let data = block
            .lines()
            .filter_map(|s| s.strip_prefix("data:").map(str::trim_start))
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            return Ok(());
        }
        let value: Value =
            serde_json::from_str(&data).map_err(|_| anyhow::anyhow!("invalid_json"))?;
        self.started |= value["type"]
            .as_str()
            .is_some_and(|kind| kind.starts_with("response."));
        match value["type"].as_str() {
            Some("response.output_text.delta") => {
                self.text.push_str(value["delta"].as_str().unwrap_or(""))
            }
            Some("response.completed") => {
                self.completed = value["response"]["status"] == "completed"
            }
            Some("response.failed" | "response.incomplete" | "error") => {
                let code = value["response"]["error"]["code"]
                    .as_str()
                    .or_else(|| value["error"]["code"].as_str())
                    .or_else(|| value["code"].as_str())
                    .unwrap_or("");
                if matches!(
                    code,
                    "subscription_sharing_usage_limit_exceeded"
                        | "subscription_sharing_usage_unavailable"
                ) {
                    bail!("openai_plan_limit");
                }
                if matches!(code, "rate_limit_exceeded" | "insufficient_quota") {
                    bail!("http_429:0");
                }
                if !code.is_empty() && code.len() <= 80 && code.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
                    bail!("openai_response_failed:{code}");
                }
                bail!("openai_response_failed");
            }
            _ => {}
        }
        ensure!(self.text.len() <= 8_000_000, "openai_response_too_large");
        Ok(())
    }
    fn finish(self) -> Result<String> {
        ensure!(self.started, "openai_stream_invalid");
        ensure!(self.completed, "openai_stream_interrupted");
        ensure!(!self.text.trim().is_empty(), "empty_response");
        Ok(self.text)
    }
}
pub async fn translate(
    engine: &Engine,
    p: &Provider,
    model: &str,
    prompt: &str,
    control: &Control,
) -> Result<String> {
    let body = json!({"model": model, "input": [{"role":"user","content":prompt}], "store":false, "stream":true});
    let response = request(engine, p, Some(&body), control).await?;
    response_text(response, control).await
}
async fn response_text(response: reqwest::Response, control: &Control) -> Result<String> {
    // Some OAuth responses omit Content-Type. Validate the actual events and
    // require response.completed before accepting any translated text.
    let mut stream = response.bytes_stream();
    let mut pending = Vec::new();
    let mut reply = ReplyStream::default();
    loop {
        let chunk = tokio::select! { v = stream.next() => v, _ = control.until_canceled() => bail!("canceled") };
        let Some(chunk) = chunk else { break };
        pending
            .extend_from_slice(&chunk.map_err(|_| anyhow::anyhow!("openai_stream_interrupted"))?);
        ensure!(pending.len() <= 8_000_000, "openai_response_too_large");
        reply.feed(&mut pending)?;
        if reply.completed {
            return reply.finish();
        }
    }
    if !pending.is_empty() {
        reply.event(std::str::from_utf8(&pending).map_err(|_| anyhow::anyhow!("invalid_json"))?)?;
    }
    reply.finish()
}

#[cfg(test)]
mod tests {
    #[test]
    fn stream_errors_keep_safe_codes_and_classify_rate_limits() {
        for (code, expected) in [("model_not_found", "openai_response_failed:model_not_found"), ("rate_limit_exceeded", "http_429:0")] {
            let event = format!("data: {}", json!({"type":"response.failed","response":{"error":{"code":code,"message":"private request details"}}}));
            let error = ReplyStream::default().event(&event).unwrap_err().to_string();
            assert_eq!(error, expected);
            assert!(!error.contains("private"));
        }
    }
    use super::*;
    #[tokio::test]
    #[ignore = "uses the saved SubForge OAuth account for one synthetic subtitle translation"]
    async fn live_oauth_subtitle_translation() {
        let dir = PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("gr.sidos.subforge");
        let settings: crate::types::Settings =
            serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap();
        let provider = settings
            .translation
            .iter()
            .find(|p| p.kind == "openai_oauth" && p.enabled)
            .unwrap();
        let file = dir.join("openai").join(format!(
            "{}.bin",
            hex::encode(Sha256::digest(provider.id.as_bytes()))
        ));
        let session: Session =
            serde_json::from_slice(&protect(&std::fs::read(file).unwrap(), true).unwrap()).unwrap();
        // Read the existing access token only; the running app owns renewal. Never print credentials.
        let cues = vec![crate::types::Cue {
            id: 1,
            start: 0.,
            end: 1.,
            text: "Hello.".into(),
        }];
        let prompt = crate::providers::translation_prompt(&cues, &[], "en", "el");
        let response = client().unwrap().post(format!("{RESOURCE}/responses"))
            .bearer_auth(&session.access_token)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .timeout(Duration::from_secs(provider.timeout_secs))
            .json(&json!({"model":provider.models[0],"input":[{"role":"user","content":prompt}],"store":false,"stream":true}))
            .send().await.unwrap();
        assert!(response.status().is_success(), "HTTP {}", response.status());
        let header_present = response.headers().contains_key("content-type");
        let text = response_text(response, &Control::default()).await.unwrap();
        let translated = crate::providers::translation_reply(&text, &cues, "el").unwrap();
        assert_eq!(translated.len(), 1);
        assert_eq!(translated[0].id, cues[0].id);
        assert_eq!(translated[0].start, cues[0].start);
        assert_eq!(translated[0].end, cues[0].end);
        assert_ne!(translated[0].text.trim(), cues[0].text);
        assert!(translated[0]
            .text
            .chars()
            .any(|c| ('\u{0370}'..='\u{03ff}').contains(&c)));
        println!("Synthetic English-to-Greek subtitle translated successfully; IDs/timings preserved; content-type present: {header_present}");
    }
    async fn mock_stream(body: &str, content_type: Option<&str>) -> reqwest::Response {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/responses", listener.local_addr().unwrap());
        let header = content_type
            .map(|v| format!("Content-Type: {v}\r\n"))
            .unwrap_or_default();
        let wire = format!(
            "HTTP/1.1 200 OK\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            socket.read(&mut request).await.unwrap();
            socket.write_all(wire.as_bytes()).await.unwrap();
        });
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn response_accepts_completed_sse_without_content_type() {
        let body = "event: response.output_text.delta\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Γεια σου.\"}\r\n\r\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
        for header in [None, Some("text/event-stream; charset=utf-8")] {
            let response = mock_stream(body, header).await;
            assert_eq!(
                response_text(response, &Control::default()).await.unwrap(),
                "Γεια σου."
            );
        }
    }
    #[tokio::test]
    async fn response_rejects_invalid_failed_and_unfinished_streams() {
        for (body, error) in [
            ("<html>Server error</html>", "openai_stream_invalid"),
            ("{\"error\":{\"message\":\"Synthetic failure\"}}", "openai_stream_invalid"),
            ("data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\ndata: [DONE]\n\n", "openai_stream_interrupted"),
            ("data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\ndata: {\"type\":\"response.incomplete\"}\n\n", "openai_response_failed"),
            ("data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"subscription_sharing_usage_limit_exceeded\"}}}\n\n", "openai_plan_limit"),
        ] {
            // A declared event-stream header must not make an invalid body acceptable.
            let response = mock_stream(body, Some("text/event-stream")).await;
            assert_eq!(response_text(response, &Control::default()).await.unwrap_err().to_string(), error);
        }
    }
    #[test]
    fn callback_checks_state_and_registration() {
        assert!(callback(
            "/auth/callback?state=bad&code=c&client_id=issued",
            "s",
            None
        )
        .is_err());
        assert!(callback(
            "/auth/callback?state=s&code=c&client_id=dynamic_agent_client",
            "s",
            None
        )
        .is_err());
        assert!(callback(
            "/auth/callback?state=s&code=c&client_id=other",
            "s",
            Some("issued")
        )
        .is_err());
        assert!(callback(
            "/auth/callback?state=s&state=s&code=c&client_id=issued",
            "s",
            None
        )
        .is_err());
        assert!(
            callback("/auth/callback?state=s&error=access_denied", "s", None)
                .unwrap_err()
                .to_string()
                .contains("consent_denied")
        );
        assert_eq!(
            callback("/auth/callback?state=s&code=c", "s", Some("issued")).unwrap(),
            ("c".into(), "issued".into())
        );
    }
    #[test]
    fn authorize_uses_pkce_and_reuses_registered_client() {
        let url = authorize_url(
            None,
            "host",
            "http://127.0.0.1:42/auth/callback",
            "state",
            "nonce",
            "verifier",
        )
        .unwrap();
        let pairs: HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs["client_id"], DYNAMIC);
        assert_eq!(pairs["agent_name_hint"], "SubForge");
        assert_eq!(
            pairs["code_challenge"],
            URL_SAFE_NO_PAD.encode(Sha256::digest(b"verifier"))
        );
        let session = Session {
            client_id: "issued".into(),
            ..Default::default()
        };
        let url = authorize_url(Some(&session), "host", "redirect", "s", "n", "v").unwrap();
        assert!(url
            .query_pairs()
            .any(|(k, v)| k == "client_id" && v == "issued"));
        assert!(!url.query_pairs().any(|(k, _)| k == "agent_name_hint"));
    }
    #[test]
    fn plan_scope_and_account_catalog_are_required() {
        assert!(!has_plan("openid email"));
        assert!(has_plan("resource.invoke chatgpt.tokens.use.direct"));
        assert_eq!(catalog(&json!({"models":[{"slug":"first","visibility":"list"},{"slug":"hidden","visibility":"hide"},{"slug":"second","visibility":"list"}]})).unwrap(), vec!["first", "second"]);
    }
    #[test]
    fn stream_requires_completed_event_and_preserves_unicode() {
        let mut stream = ReplyStream::default();
        stream
            .event("data: {\"type\":\"response.output_text.delta\",\"delta\":\"Καλημέρα\"}\r\n\r\n")
            .unwrap();
        assert!(stream.finish().is_err());
        let mut stream = ReplyStream::default();
        stream
            .event("data: {\"type\":\"response.output_text.delta\",\"delta\":\"Καλημέρα\"}\n\n")
            .unwrap();
        stream.event("data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n").unwrap();
        assert_eq!(stream.finish().unwrap(), "Καλημέρα");
        assert!(ReplyStream::default().event("data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"subscription_sharing_usage_limit_exceeded\"}}}").unwrap_err().to_string().contains("plan_limit"));
    }
    #[test]
    fn stream_handles_fragmented_unicode_and_mixed_sse_newlines() {
        let wire = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Καλημέρα\"}\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\" κόσμε\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\r\n\r\n";
        for chunk_size in [1, 7, wire.len()] {
            let mut reply = ReplyStream::default();
            let mut pending = Vec::new();
            for chunk in wire.as_bytes().chunks(chunk_size) {
                pending.extend_from_slice(chunk);
                reply.feed(&mut pending).unwrap();
            }
            assert_eq!(reply.finish().unwrap(), "Καλημέρα κόσμε");
        }
    }
    #[test]
    fn credentials_are_encrypted_with_windows_account() {
        let secret = b"synthetic-oauth-credential";
        let encrypted = protect(secret, false).unwrap();
        assert!(!encrypted.windows(secret.len()).any(|w| w == secret));
        assert_eq!(protect(&encrypted, true).unwrap(), secret);
    }
}
