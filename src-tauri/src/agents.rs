//! SubForge owns runtime installation and every agent process. No PATH or user
//! configuration is changed; downloaded programs run only after hash validation.
use crate::{
    engine::{Control, Engine},
    media, providers,
    types::Provider,
};
use anyhow::{bail, ensure, Result};
use futures_util::StreamExt;
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use sha2::{Digest, Sha256, Sha512};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

struct Runtime {
    version: &'static str,
    url: &'static str,
    hash: &'static str,
    binary: &'static str,
    archive: bool,
}
fn runtime(kind: &str) -> Result<Runtime> {
    let kind = runtime_kind(kind);
    Ok(match kind {
        "antigravity" => Runtime { version:"1.3.3", url:"https://storage.googleapis.com/antigravity-public/antigravity-cli/1.3.3-5524738307653632/windows-x64/cli_windows_x64.exe", hash:"ec15855bd5de768232c0aee31d11eac636a9e7d8bc4ad3dbe3887a9797f7fd821cafabba61482cd5515674a05246be2b7f96ca3f17e38c6a3f7431a2a3aff81f", binary:"agy.exe", archive:false },
        "opencode" => Runtime { version:"1.19.0", url:"https://github.com/anomalyco/opencode/releases/download/v1.19.0/opencode-windows-x64-baseline.zip", hash:"2759ca3449fe946d82da3be43d2e85332127399686aafceaff153b90c3079c56", binary:"opencode.exe", archive:true },
        "pi" => Runtime { version:"1.1.0", url:"https://github.com/earendil-works/pi/releases/download/v1.1.0/pi-windows-x64.zip", hash:"98e3a14e13685a4836a10865bd7887bdc2fcfa1086120313420dc935a401d3d6", binary:"pi.exe", archive:true },
        _ => bail!("unsupported_agent"),
    })
}
fn runtime_kind(kind: &str) -> &str {
    if kind == "zen" {
        "opencode"
    } else {
        kind
    }
}
pub fn binary(dir: &Path, kind: &str) -> Result<PathBuf> {
    let r = runtime(kind)?;
    Ok(dir
        .join("agents/runtimes")
        .join(runtime_kind(kind))
        .join(r.version)
        .join(r.binary))
}
pub fn ready(dir: &Path, kind: &str) -> bool {
    runtime(kind).is_ok_and(|r| {
        binary(dir, kind).is_ok_and(|p| {
            p.exists()
                && fs::read_to_string(p.with_extension("ready")).is_ok_and(|hash| hash == r.hash)
        })
    })
}
static INSTALL_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
pub async fn prepare(engine: &Engine, kind: &str, control: &Control) -> Result<()> {
    let lock = INSTALL_LOCK.get_or_init(Default::default);
    let _guard = tokio::select! {g=lock.lock()=>g,_=control.until_canceled()=>bail!("canceled")};
    if ready(&engine.dir, kind) {
        return Ok(());
    }
    let r = runtime(kind)?;
    let dest = binary(&engine.dir, kind)?;
    let parent = dest.parent().unwrap();
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!("download-{}.part", uuid::Uuid::new_v4()));
    let result=async {
        let c=reqwest::Client::builder().timeout(std::time::Duration::from_secs(600)).user_agent("SubForge/0.1.6").build()?;
        let response=tokio::select!{r=c.get(r.url).send()=>r.map_err(|_|anyhow::anyhow!("agent_download_failed"))?,_=control.until_canceled()=>bail!("canceled")};
        ensure!(response.status().is_success(),"agent_download_failed");
        let mut file=fs::File::create(&staging)?;
        let mut stream=response.bytes_stream();let mut total=0u64;
        let mut hash256=Sha256::new();let mut hash512=Sha512::new();
        loop {
            let part=tokio::select!{p=stream.next()=>p,_=control.until_canceled()=>bail!("canceled")};
            let Some(part)=part else {break};let bytes=part.map_err(|_|anyhow::anyhow!("agent_download_failed"))?;
            total+=bytes.len() as u64;ensure!(total<700_000_000,"agent_download_too_large");
            file.write_all(&bytes)?;hash256.update(&bytes);hash512.update(&bytes);
        }
        file.sync_all()?;drop(file);
        let actual=if r.hash.len()==128 {hex::encode(hash512.finalize())}else{hex::encode(hash256.finalize())};
        ensure!(actual==r.hash,"agent_checksum_failed");
        if r.archive {extract(&staging,parent)?;ensure!(dest.exists(),"agent_archive_invalid");}
        else {fs::rename(&staging,&dest)?;}
        let mut cmd=tokio::process::Command::new(&dest);cmd.arg("--version");
        media::command(cmd,control,30).await?;
        crate::engine::atomic(&dest.with_extension("ready"),r.hash.as_bytes())?;
        Ok(())
    }.await;
    let _ = fs::remove_file(&staging);
    if result.is_err() {
        let _ = fs::remove_file(&dest);
    }
    engine.emit();
    result
}
fn extract(archive: &Path, dest: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| anyhow::anyhow!("agent_archive_invalid"))?;
        ensure!(!entry.is_symlink(), "agent_archive_invalid");
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("agent_archive_invalid"))?;
        ensure!(total < 1_500_000_000, "agent_archive_too_large");
        let path = dest.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(path)?;
        } else {
            fs::create_dir_all(path.parent().unwrap())?;
            let mut file = fs::File::create(path)?;
            std::io::copy(&mut entry, &mut file)?;
        }
    }
    Ok(())
}
pub fn profile(dir: &Path, p: &Provider) -> PathBuf {
    // IDs come from imported settings as well as UUIDs; never use them as paths.
    dir.join("agents/profiles")
        .join(&p.kind)
        .join(hex::encode(Sha256::digest(p.id.as_bytes())))
        .join(if p.auth == "oauth" { "oauth" } else { "key" })
}
pub fn command(dir: &Path, p: &Provider) -> Result<tokio::process::Command> {
    let exe = binary(dir, &p.kind)?;
    ensure!(ready(dir, &p.kind), "agent_setup_required");
    let home = profile(dir, p);
    fs::create_dir_all(&home)?;
    let mut cmd = tokio::process::Command::new(exe);
    match p.kind.as_str() {
        "antigravity" => {
            configure_antigravity(&home, p)?;
            cmd.env("USERPROFILE", &home)
                .env("HOME", &home)
                .env("AGY_CLI_DISABLE_AUTO_UPDATE", "1");
            if p.auth == "key" {
                cmd.env("GEMINI_API_KEY", providers::key(&p.id)?);
            } else {
                cmd.env_remove("GEMINI_API_KEY");
            }
            if !p.endpoint.is_empty() {
                cmd.env("GOOGLE_GEMINI_BASE_URL", p.endpoint.trim_end_matches('/'));
            } else {
                cmd.env_remove("GOOGLE_GEMINI_BASE_URL");
            }
        }
        "opencode" | "zen" => {
            cmd.env("XDG_CONFIG_HOME", home.join("config"))
                .env("XDG_DATA_HOME", home.join("data"))
                .env("XDG_CACHE_HOME", home.join("cache"))
                .env("OPENCODE_DISABLE_AUTOUPDATE", "true")
                .env_remove("OPENCODE_CONFIG")
                .env_remove("OPENCODE_CONFIG_DIR")
                .env_remove("OPENCODE_PERMISSION")
                .env_remove("OPENCODE_API_KEY")
                .current_dir(&home);
            // Zen free models require the normal CLI tool schemas. `run --no-auto`
            // auto-rejects every permission request; blanket deny removes schemas
            // and makes the service reject even official CLI requests with 403.
            let mut config =
                serde_json::json!({"permission":"ask","autoupdate":false,"share":"disabled"});
            if let Ok(token) = providers::key(&p.id) {
                cmd.env("OPENCODE_API_KEY", token);
                // Resolve the saved key only inside the child. No key is persisted.
                config["provider"] =
                    serde_json::json!({"opencode":{"options":{"apiKey":"{env:OPENCODE_API_KEY}"}}});
            }
            cmd.env("OPENCODE_CONFIG_CONTENT", serde_json::to_string(&config)?);
        }
        "pi" => {
            cmd.env("PI_CODING_AGENT_DIR", &home);
        }
        _ => bail!("unsupported_agent"),
    }
    Ok(cmd)
}
fn configure_antigravity(home: &Path, p: &Provider) -> Result<()> {
    let path = home.join(".gemini/antigravity-cli/settings.json");
    fs::create_dir_all(path.parent().unwrap())?;
    // Separate key and OAuth profiles: switching never edits the user's CLI files.
    let mut config: serde_json::Value = if path.exists() {
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        serde_json::json!({})
    };
    ensure!(config.is_object(), "agent_config_invalid");
    config["permissions"] = serde_json::json!({"deny":["command(*)","write_file(*)"]});
    if p.auth == "key" {
        config["modelProvider"] = serde_json::json!("gemini");
    } else {
        config.as_object_mut().unwrap().remove("modelProvider");
    }
    crate::engine::atomic(&path, &serde_json::to_vec_pretty(&config)?)?;
    Ok(())
}

pub struct Login {
    provider: Provider,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    _master: Box<dyn MasterPty + Send>,
    output: Arc<Mutex<String>>,
    started: std::time::Instant,
    advanced: std::collections::HashSet<&'static str>,
    terms_pending: bool,
    browser_url: Option<String>,
    browser_opened: bool,
    setup_inputs: std::collections::VecDeque<Vec<u8>>,
    next_input: std::time::Instant,
}
// ConPTY forwards cursor-position queries to its frontend. Without an answer,
// the native Antigravity TUI never renders and never reaches Google sign-in.
fn terminal_replies(pending: &mut Vec<u8>, chunk: &[u8]) -> Vec<u8> {
    const QUERIES: [(&[u8], &[u8]); 2] = [(b"\x1b[6n", b"\x1b[1;1R"), (b"\x1b[?6n", b"\x1b[?1;1R")];
    pending.extend_from_slice(chunk);
    let mut replies = Vec::new();
    for (query, reply) in QUERIES {
        for _ in pending.windows(query.len()).filter(|part| *part == query) {
            replies.extend_from_slice(reply);
        }
    }
    let keep = QUERIES
        .iter()
        .flat_map(|(q, _)| 1..q.len())
        .filter(|&n| {
            QUERIES
                .iter()
                .any(|(q, _)| n < q.len() && pending.ends_with(&q[..n]))
        })
        .max()
        .unwrap_or(0);
    pending.drain(..pending.len() - keep);
    replies
}
fn signin_text(raw: &str) -> Result<String> {
    let plain = regex::Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]")?.replace_all(raw, "");
    Ok(regex::Regex::new(r"\x1b\][^\x07]*(?:\x07|\x1b\\)")?
        .replace_all(&plain, "")
        .into_owned())
}
fn google_auth_url(text: &str) -> Option<String> {
    let pattern = regex::Regex::new(r#"https://[^\x00-\x20<>\"']+"#).ok()?;
    let result = pattern.find_iter(text).find_map(|found| {
        let url = reqwest::Url::parse(found.as_str()).ok()?;
        (url.host_str() == Some("accounts.google.com")
            && url.path().starts_with("/o/oauth2/")
            && url.query_pairs().any(|(key, _)| key == "client_id")
            && url.query_pairs().any(|(key, _)| key == "redirect_uri")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none())
        .then(|| url.to_string())
    });
    result
}
fn open_signin_browser(url: &str) -> Result<()> {
    ensure!(
        google_auth_url(url).as_deref() == Some(url),
        "signin_url_invalid"
    );
    open_browser(url)
}
pub(crate) fn open_browser(url: &str) -> Result<()> {
    let parsed = reqwest::Url::parse(url).map_err(|_| anyhow::anyhow!("signin_url_invalid"))?;
    ensure!(matches!(parsed.scheme(), "https" | "http") && !url.contains('\0'), "signin_url_invalid");
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                hwnd: isize,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show: i32,
            ) -> isize;
        }
        let operation: Vec<u16> = std::ffi::OsStr::new("open")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let url: Vec<u16> = std::ffi::OsStr::new(url)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let status = unsafe {
            ShellExecuteW(
                0,
                operation.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        ensure!(status > 32, "signin_browser_failed");
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        anyhow::bail!("signin_browser_unavailable")
    }
}
fn legal_url(text: &str, kind: &str) -> Option<String> {
    let pattern = regex::Regex::new(r#"https://[^\s<>\"']+"#).ok()?;
    let result = pattern.find_iter(text).find_map(|found| {
        let url = reqwest::Url::parse(found.as_str()).ok()?;
        (matches!(
            url.host_str(),
            Some("antigravity.google" | "policies.google.com")
        ) && url.path().contains(kind)
            && url.query().is_none()
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none())
        .then(|| url.to_string())
    });
    result
}
impl Drop for Login {
    fn drop(&mut self) {
        if let Some(pid) = self.child.process_id() {
            let mut cmd = std::process::Command::new("taskkill.exe");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            let _ = cmd
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        let _ = self.child.kill();
    }
}
pub fn login(engine: &Engine, p: &Provider) -> Result<()> {
    ensure!(
        p.kind == "antigravity" && p.auth == "oauth",
        "oauth_provider_required"
    );
    let home = profile(&engine.dir, p);
    configure_antigravity(&home, p)?;
    let pair = portable_pty::native_pty_system().openpty(PtySize {
        rows: 28,
        cols: 100,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    let mut cmd = CommandBuilder::new(binary(&engine.dir, &p.kind)?);
    cmd.cwd(&home);
    cmd.env("USERPROFILE", &home);
    cmd.env("HOME", &home);
    cmd.env_remove("GEMINI_API_KEY");
    cmd.env("AGY_CLI_DISABLE_AUTO_UPDATE", "1");
    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    let terminal_writer = writer.clone();
    let output = Arc::new(Mutex::new(String::new()));
    let collected = output.clone();
    std::thread::spawn(move || {
        let mut bytes = [0; 8192];
        let mut pending = Vec::new();
        while let Ok(size) = reader.read(&mut bytes) {
            if size == 0 {
                break;
            }
            let replies = terminal_replies(&mut pending, &bytes[..size]);
            if !replies.is_empty() {
                let mut writer = terminal_writer.lock().unwrap();
                if writer
                    .write_all(&replies)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
            let mut out = collected.lock().unwrap();
            out.push_str(&String::from_utf8_lossy(&bytes[..size]));
            if out.len() > 65536 {
                out.clear();
            }
        }
    });
    engine.logins.lock().unwrap().insert(
        p.id.clone(),
        Login {
            provider: p.clone(),
            child,
            writer,
            _master: pair.master,
            output,
            started: std::time::Instant::now(),
            advanced: Default::default(),
            terms_pending: false,
            browser_url: None,
            browser_opened: false,
            setup_inputs: Default::default(),
            next_input: std::time::Instant::now(),
        },
    );
    Ok(())
}
pub fn login_action(engine: &Engine, id: &str, action: &str) -> Result<String> {
    let mut logins = engine.logins.lock().unwrap();
    if action == "finish" || action == "cancel" {
        logins.remove(id);
        return Ok("closed".into());
    }
    let login = logins
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("signin_not_running"))?;
    ensure!(
        matches!(
            action,
            "continue" | "poll" | "open_browser" | "open_terms" | "open_privacy" | "switch_account"
        ),
        "signin_action_invalid"
    );
    if login.child.try_wait()?.is_some() {
        if login.advanced.contains("logout_requested") {
            let provider = login.provider.clone();
            logins.remove(id);
            drop(logins);
            self::login(engine, &provider)?;
            return Ok("signin_waiting".into());
        }
        return Ok("signin_failed".into());
    }
    if login.started.elapsed() > std::time::Duration::from_secs(600) {
        logins.remove(id);
        return Ok("signin_timeout".into());
    }
    let raw = login.output.lock().unwrap().clone();
    let plain = signin_text(&raw)?;
    if action == "switch_account" {
        ensure!(native_signed_in(&plain), "signin_not_connected");
        // This is exposed only as an explicit account-switch action. Never
        // purge native credentials merely because the sign-in button was used.
        login.advanced.insert("logout_requested");
        login.browser_url = None;
        login.browser_opened = false;
        login.output.lock().unwrap().clear();
        let mut writer = login.writer.lock().unwrap();
        writer.write_all(b"\x15/logout\r")?;
        writer.flush()?;
        return Ok("signin_waiting".into());
    }
    let mut input: &[u8] = b"";
    if matches!(action, "open_terms" | "open_privacy") {
        let kind = if action == "open_terms" {
            "terms"
        } else {
            "privacy"
        };
        let url =
            legal_url(&plain, kind).ok_or_else(|| anyhow::anyhow!("signin_url_unavailable"))?;
        open_browser(&url)?;
        return Ok("signin_terms".into());
    }
    if action == "open_browser" {
        let url = login
            .browser_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("signin_url_unavailable"))?;
        open_signin_browser(url)?;
        login.browser_opened = true;
        return Ok("signin_browser_ready".into());
    }
    if !login.setup_inputs.is_empty() {
        if std::time::Instant::now() >= login.next_input {
            let keys = login.setup_inputs.pop_front().unwrap();
            let mut writer = login.writer.lock().unwrap();
            writer.write_all(&keys)?;
            writer.flush()?;
            login.next_input = std::time::Instant::now() + std::time::Duration::from_millis(300);
        }
        return Ok("signin_waiting".into());
    }
    if login.terms_pending {
        if action != "continue" {
            return Ok("signin_terms".into());
        }
        // The user explicitly clicked Continue after reviewing the provider terms.
        // The unchecked data-sharing option remains off; move to Done and confirm.
        login.terms_pending = false;
        // Send navigation separately so a TUI redraw cannot swallow the next key.
        login.setup_inputs = [b"\t".to_vec(), b"\r".to_vec()].into();
        login.next_input = std::time::Instant::now() + std::time::Duration::from_millis(300);
        input = b"\x1b[B";
    } else if plain.contains("Terms of Service & Data Use") && !login.advanced.contains("terms") {
        login.advanced.insert("terms");
        login.terms_pending = true;
        if plain.contains("[x]") {
            let mut writer = login.writer.lock().unwrap();
            writer.write_all(b"\r")?;
            writer.flush()?;
        }
        return Ok("signin_terms".into());
    } else if plain.contains("Choose your color scheme") && login.advanced.insert("theme") {
        input = b"\r";
    } else if plain.contains("Choose your rendering mode") && login.advanced.insert("rendering") {
        input = b"\r";
    } else if action == "continue" {
        input = b"\r";
    }
    if !input.is_empty() {
        login.output.lock().unwrap().clear();
        let mut writer = login.writer.lock().unwrap();
        writer.write_all(input)?;
        writer.flush()?;
        return Ok("signin_waiting".into());
    }
    // Native CLI links can exist only in OSC hyperlink sequences. Inspect the
    // raw bounded buffer, not the display text with its hyperlinks stripped.
    if let Some(url) = google_auth_url(&raw) {
        login.browser_url = Some(url);
        if !login.browser_opened {
            open_signin_browser(login.browser_url.as_deref().unwrap())?;
            login.browser_opened = true;
        }
        return Ok("signin_browser_ready".into());
    }
    if native_signed_in(&plain) {
        return Ok("signin_connected".into());
    }
    // Terminal text and authorization URLs never enter GUI responses or logs.
    Ok(
        if plain.to_lowercase().contains("authentication required") {
            "signin_needed"
        } else if plain.to_lowercase().contains("browser") {
            "signin_browser"
        } else {
            "signin_waiting"
        }
        .into(),
    )
}
fn native_signed_in(text: &str) -> bool {
    !text.contains("not signed in")
        && (text.contains("Antigravity") || text.contains("Gemini"))
        && regex::Regex::new(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
            .is_ok_and(|pattern| pattern.is_match(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_rejects_non_web_targets_and_embedded_nul() {
        for url in ["file:///C:/Users", "javascript:alert(1)", "https://example.com/\0ignored"] {
            assert!(open_browser(url).is_err());
        }
    }
    #[test]
    #[ignore = "opens the actual Windows default browser to a local test page"]
    fn default_browser_receives_complete_oauth_shaped_url() {
        use std::{net::TcpListener, time::{Duration, Instant}};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let state = uuid::Uuid::new_v4().to_string();
        let hint = format!("synthetic-{}", "x".repeat(5000));
        let mut url = reqwest::Url::parse(&format!("{base}/auth/callback")).unwrap();
        url.query_pairs_mut().extend_pairs([
            ("state", state.as_str()), ("client_id", "synthetic-client"),
            ("scope", "openid profile email"), ("code", "synthetic&code+with=punctuation"),
            ("id_token_hint", hint.as_str()),
        ]);
        open_browser(url.as_str()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => panic!("default browser did not navigate to the callback: {e}"),
            }
        };
        socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut bytes = Vec::new();
        while bytes.len() < 16384 && !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut buffer = [0u8; 1024]; let count = socket.read(&mut buffer).unwrap();
            if count == 0 { break; } bytes.extend_from_slice(&buffer[..count]);
        }
        let body = "SubForge browser test passed. You can close this tab.";
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        let request = std::str::from_utf8(&bytes).unwrap();
        let target = request.lines().next().unwrap().strip_prefix("GET ").unwrap().strip_suffix(" HTTP/1.1").unwrap();
        let returned = reqwest::Url::parse(&format!("{base}{target}")).unwrap();
        assert_eq!(returned.path(), "/auth/callback");
        let pairs: std::collections::HashMap<_, _> = returned.query_pairs().collect();
        assert_eq!(pairs["state"], state); assert_eq!(pairs["client_id"], "synthetic-client");
        assert_eq!(pairs["scope"], "openid profile email");
        assert_eq!(pairs["code"], "synthetic&code+with=punctuation"); assert_eq!(pairs["id_token_hint"], hint);
    }
    #[test]
    fn hidden_terminal_answers_split_cursor_queries() {
        let mut pending = Vec::new();
        assert!(terminal_replies(&mut pending, b"intro\x1b[").is_empty());
        assert_eq!(terminal_replies(&mut pending, b"6n"), b"\x1b[1;1R");
        assert!(terminal_replies(&mut pending, b"text").is_empty());
        assert_eq!(
            terminal_replies(&mut pending, b"\x1b[?6n\x1b[6n"),
            b"\x1b[1;1R\x1b[?1;1R"
        );
        assert!(pending.is_empty());
    }
    #[test]
    fn browser_links_only_accept_google_oauth_and_public_legal_pages() {
        let url = "https://accounts.google.com/o/oauth2/v2/auth?client_id=fixture&redirect_uri=http%3A%2F%2Flocalhost%3A12345&state=synthetic";
        assert_eq!(google_auth_url(&format!("Open {url}")), Some(url.into()));
        assert_eq!(
            google_auth_url(&format!("\x1b]8;;{url}\x1b\\Sign in\x1b]8;;\x1b\\")),
            Some(url.into())
        );
        assert!(google_auth_url(
            "https://accounts.google.com.evil/o/oauth2/auth?client_id=fixture"
        )
        .is_none());
        assert!(google_auth_url(
            "https://user@accounts.google.com/o/oauth2/auth?client_id=fixture"
        )
        .is_none());
        assert!(
            google_auth_url("http://accounts.google.com/o/oauth2/auth?client_id=fixture").is_none()
        );
        assert!(google_auth_url("https://accounts.google.com/other?client_id=fixture").is_none());
        assert_eq!(
            legal_url("https://antigravity.google/terms", "terms"),
            Some("https://antigravity.google/terms".into())
        );
        assert!(legal_url("https://evil.test/terms", "terms").is_none());
    }
    #[test]
    fn native_account_status_is_detected_without_returning_identity() {
        assert!(native_signed_in(
            "Antigravity CLI account@example.test Gemini 3.8 Flash"
        ));
        assert!(!native_signed_in(
            "Antigravity CLI: not signed in. account@example.test"
        ));
        assert!(!native_signed_in("Terms of Service & Data Use"));
    }
    #[test]
    fn configuring_native_cli_preserves_setup_preferences() {
        let home = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let path = home.join(".gemini/antigravity-cli/settings.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            b"{\"theme\":\"dark\",\"modelProvider\":\"gemini\",\"setupCompleted\":true}",
        )
        .unwrap();
        let p = Provider {
            kind: "antigravity".into(),
            auth: "oauth".into(),
            ..Default::default()
        };
        configure_antigravity(&home, &p).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["theme"], "dark");
        assert_eq!(saved["setupCompleted"], true);
        assert!(saved.get("modelProvider").is_none());
        fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn zen_reuses_the_managed_opencode_runtime() {
        let dir = Path::new(r"E:\SubForge");
        assert_eq!(
            binary(dir, "zen").unwrap(),
            binary(dir, "opencode").unwrap()
        );
        assert_eq!(
            runtime("zen").unwrap().hash,
            runtime("opencode").unwrap().hash
        );
    }
    #[test]
    fn imported_id_cannot_escape_profile() {
        let root = Path::new(r"E:\SubForge");
        let p = Provider {
            id: "../../../foreign".into(),
            kind: "antigravity".into(),
            ..Default::default()
        };
        assert!(profile(root, &p).starts_with(root));
        assert_eq!(
            profile(root, &p)
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .len(),
            64
        );
    }
    #[test]
    fn incomplete_agent_install_cannot_be_used() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let exe = binary(&dir, "pi").unwrap();
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(&exe, b"incomplete fixture").unwrap();
        assert!(!ready(&dir, "pi"));
        fs::write(exe.with_extension("ready"), b"wrong checksum").unwrap();
        assert!(!ready(&dir, "pi"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn archive_cannot_write_outside_runtime_directory() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("fixture.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        writer
            .start_file("../outside.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"fixture").unwrap();
        writer.finish().unwrap();
        assert!(extract(&archive, &dir.join("runtime")).is_err());
        assert!(!dir.join("outside.exe").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
