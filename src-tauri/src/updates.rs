//! GitHub release installer updates, verified against GitHub's asset SHA-256.
use crate::engine::Engine;
use anyhow::{bail, ensure, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::os::windows::process::CommandExt;
use std::{
    path::Path,
    sync::{atomic::Ordering, Mutex},
    time::Duration,
};
use tauri::Emitter;
use tokio::io::AsyncWriteExt;

const REPO: &str = "sidosgr/SubForge";
const LATEST: &str = "https://api.github.com/repos/sidosgr/SubForge/releases/latest";
const INSTALLER: &str = "SubForge-Setup-x64.exe";
const MAX_SIZE: u64 = 2_000_000_000;

#[derive(Default)]
pub struct Updater {
    lock: tokio::sync::Mutex<()>,
    status: Mutex<Progress>,
}
#[derive(Clone, Default, Serialize)]
pub struct Progress {
    pub stage: String,
    pub downloaded: u64,
    pub total: u64,
}
#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
    state: String,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[derive(Serialize)]
pub struct Available {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub installable: bool,
    pub notes: String,
    pub published_at: String,
    pub url: String,
    pub size: u64,
}
fn version(tag: &str) -> Result<semver::Version> {
    let version = semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag))
        .map_err(|_| anyhow::anyhow!("update_version_invalid"))?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "update_version_invalid"
    );
    Ok(version)
}
fn digest(asset: &Asset) -> Result<String> {
    let sha = asset
        .digest
        .as_deref()
        .and_then(|s| s.strip_prefix("sha256:"))
        .unwrap_or("");
    ensure!(
        sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "update_verification_unavailable"
    );
    Ok(sha.to_ascii_lowercase())
}
fn installer(release: &Release) -> Result<Asset> {
    version(&release.tag_name)?;
    ensure!(
        !release.draft && !release.prerelease,
        "update_release_invalid"
    );
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == INSTALLER && a.state == "uploaded")
        .ok_or_else(|| anyhow::anyhow!("update_installer_unavailable"))?;
    let expected = format!(
        "https://github.com/{REPO}/releases/download/{}/{INSTALLER}",
        release.tag_name
    );
    ensure!(
        asset.browser_download_url == expected && asset.size > 0 && asset.size <= MAX_SIZE,
        "update_asset_invalid"
    );
    digest(asset)?;
    Ok(asset.clone())
}
fn describe(release: &Release, current: &str) -> Result<Available> {
    let latest = version(&release.tag_name)?;
    ensure!(
        !release.draft && !release.prerelease,
        "update_release_invalid"
    );
    let canonical = format!(
        "https://github.com/{REPO}/releases/tag/{}",
        release.tag_name
    );
    ensure!(release.html_url == canonical, "update_release_invalid");
    let asset = installer(release).ok();
    Ok(Available {
        current_version: current.into(),
        latest_version: latest.to_string(),
        available: latest > version(current)?,
        installable: asset.is_some(),
        notes: release.body.clone().unwrap_or_default(),
        published_at: release.published_at.clone().unwrap_or_default(),
        url: canonical,
        size: asset.map(|a| a.size).unwrap_or(0),
    })
}
fn client(timeout: u64) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let allowed = matches!(
                attempt.url().host_str(),
                Some(
                    "github.com"
                        | "release-assets.githubusercontent.com"
                        | "objects.githubusercontent.com"
                )
            );
            if attempt.previous().len() > 5 || attempt.url().scheme() != "https" || !allowed {
                attempt.error("update_redirect_invalid")
            } else {
                attempt.follow()
            }
        }))
        .user_agent(concat!("SubForge/", env!("CARGO_PKG_VERSION")))
        .build()?)
}
async fn latest() -> Result<Option<Release>> {
    let response = client(30)?
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("update_network_error"))?;
    if response.status() == 404 {
        return Ok(None);
    }
    ensure!(
        response.status().is_success(),
        "update_check_failed:{}",
        response.status().as_u16()
    );
    let value = response
        .json()
        .await
        .map_err(|_| anyhow::anyhow!("update_metadata_invalid"))?;
    Ok(Some(value))
}
pub async fn check() -> Result<Available> {
    let current = env!("CARGO_PKG_VERSION");
    match latest().await? {
        Some(release) => describe(&release, current),
        None => Ok(Available {
            current_version: current.into(),
            latest_version: current.into(),
            available: false,
            installable: false,
            notes: String::new(),
            published_at: String::new(),
            url: format!("https://github.com/{REPO}/releases"),
            size: 0,
        }),
    }
}
pub fn status(engine: &Engine) -> Progress {
    engine.updater.status.lock().unwrap().clone()
}
fn progress(engine: &Engine, stage: &str, downloaded: u64, total: u64) {
    let progress = Progress {
        stage: stage.into(),
        downloaded,
        total,
    };
    *engine.updater.status.lock().unwrap() = progress.clone();
    let _ = engine.app.emit("update-progress", progress);
}
fn verify(actual: &[u8], expected: &str, downloaded: u64, size: u64) -> Result<()> {
    ensure!(
        downloaded == size && hex::encode(actual) == expected,
        "update_verification_failed"
    );
    Ok(())
}
async fn download(engine: &Engine, asset: &Asset, expected: &str, path: &Path) -> Result<()> {
    let response = client(1800)?
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("update_download_failed"))?;
    ensure!(response.status().is_success(), "update_download_failed");
    if let Some(length) = response.content_length() {
        ensure!(length == asset.size, "update_size_mismatch");
    }
    let mut file = tokio::fs::File::create(path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;
    let mut hasher = Sha256::new();
    let mut last_event = std::time::Instant::now();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(60), stream.next())
        .await
        .map_err(|_| anyhow::anyhow!("update_download_timeout"))?
    {
        let chunk = chunk.map_err(|_| anyhow::anyhow!("update_download_failed"))?;
        downloaded += chunk.len() as u64;
        ensure!(downloaded <= asset.size, "update_size_mismatch");
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        if last_event.elapsed() >= Duration::from_millis(200) {
            progress(engine, "downloading", downloaded, asset.size);
            last_event = std::time::Instant::now();
        }
    }
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    progress(engine, "verifying", downloaded, asset.size);
    verify(&hasher.finalize(), expected, downloaded, asset.size)
}
async fn verify_file(mut file: tokio::fs::File, expected: &str, size: u64) -> Result<()> {
    // Read again while the executable is locked against writes and replacement.
    use tokio::io::AsyncReadExt;
    let mut hasher = Sha256::new();
    let mut length = 0;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        length += count as u64;
    }
    verify(&hasher.finalize(), expected, length, size)
}
struct Maintenance<'a>(&'a Engine);
impl Drop for Maintenance<'_> {
    fn drop(&mut self) {
        self.0.maintenance.store(false, Ordering::SeqCst);
    }
}
fn reserve_install(engine: &Engine) -> Result<Maintenance<'_>> {
    let active = engine.active.lock().unwrap();
    ensure!(active.is_empty(), "update_jobs_active");
    let downloads = engine.downloads.lock().unwrap();
    ensure!(!downloads.values().any(|s| s.downloading), "update_jobs_active");
    engine.maintenance.store(true, Ordering::SeqCst);
    Ok(Maintenance(engine))
}
pub async fn install(engine: &Engine, expected_version: &str) -> Result<()> {
    let _lock = engine
        .updater
        .lock
        .try_lock()
        .map_err(|_| anyhow::anyhow!("update_in_progress"))?;
    progress(engine, "checking", 0, 0);
    let result = install_inner(engine, expected_version).await;
    if result.is_err() {
        progress(engine, "failed", 0, 0);
    }
    result
}
async fn install_inner(engine: &Engine, expected_version: &str) -> Result<()> {
    if cfg!(debug_assertions) {
        bail!("update_requires_installed_build");
    }
    ensure!(
        engine.active.lock().unwrap().is_empty()
            && !engine
                .downloads
                .lock()
                .unwrap()
                .values()
                .any(|s| s.downloading),
        "update_jobs_active"
    );
    let release = latest()
        .await?
        .ok_or_else(|| anyhow::anyhow!("update_not_available"))?;
    let info = describe(&release, env!("CARGO_PKG_VERSION"))?;
    ensure!(
        info.available && info.latest_version == expected_version,
        "update_release_changed"
    );
    let asset = installer(&release)?;
    let expected = digest(&asset)?;
    let folder = engine.dir.join("updates");
    std::fs::create_dir_all(&folder)?;
    let temporary = folder.join(format!("{}.part", uuid::Uuid::new_v4()));
    progress(engine, "downloading", 0, asset.size);
    let result = download(engine, &asset, &expected, &temporary).await;
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    // Downloading can happen during work; installation requires an idle worker.
    let maintenance = match reserve_install(engine) {
        Ok(value) => value,
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            return Err(e);
        }
    };
    let executable = folder.join(format!(
        "SubForge-{}-{}.exe",
        info.latest_version,
        uuid::Uuid::new_v4()
    ));
    std::fs::rename(&temporary, &executable)?;
    // Hold a read-only Windows handle that denies write/delete until process creation.
    use std::os::windows::fs::OpenOptionsExt;
    let guard = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&executable)?;
    verify_file(
        tokio::fs::File::from_std(guard.try_clone()?),
        &expected,
        asset.size,
    )
    .await?;
    progress(engine, "installing", asset.size, asset.size);
    let mut command = std::process::Command::new(&executable);
    command.args(["/P", "/UPDATE", "/R"]);
    // NSIS /D must be last and unquoted; preserve custom installation locations.
    let current_exe = std::env::current_exe()?;
    let directory = current_exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("update_install_path_invalid"))?;
    command.raw_arg(format!("/D={}", directory.display()));
    command
        .spawn()
        .map_err(|_| anyhow::anyhow!("update_launch_failed"))?;
    drop(guard);
    crate::openai_oauth::cancel_all(engine);
    engine.logins.lock().unwrap().clear();
    engine.save();
    // Keep the scheduling gate closed until Tauri exits.
    std::mem::forget(maintenance);
    engine.app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release() -> Release {
        Release { tag_name: "v0.1.10".into(), html_url: "https://github.com/sidosgr/SubForge/releases/tag/v0.1.10".into(),
            body: Some("Release notes".into()), published_at: None, draft: false, prerelease: false,
            assets: vec![Asset { name: INSTALLER.into(), browser_download_url: "https://github.com/sidosgr/SubForge/releases/download/v0.1.10/SubForge-Setup-x64.exe".into(),
                size: 100, digest: Some(format!("sha256:{}", "a".repeat(64))), state: "uploaded".into() }] }
    }
    #[test]
    fn checks_semver_and_ignores_old_or_preview_releases() {
        let mut r = release();
        assert!(describe(&r, "0.1.9").unwrap().available);
        assert!(!describe(&r, "0.1.10").unwrap().available);
        assert!(!describe(&r, "0.2.0").unwrap().available);
        r.prerelease = true;
        assert!(describe(&r, "0.1.9").is_err());
        assert!(version("v0.2.0-beta").is_err());
        assert!(version("../../setup").is_err());
    }
    #[test]
    fn rejects_untrusted_assets_and_missing_checksums() {
        let mut r = release();
        assert!(installer(&r).is_ok());
        r.assets[0].browser_download_url = "https://evil.example/SubForge-Setup-x64.exe".into();
        assert!(installer(&r).is_err());
        r = release();
        r.assets[0].digest = None;
        assert!(installer(&r).is_err());
        assert!(!describe(&r, "0.1.9").unwrap().installable);
        r = release();
        r.assets[0].size = MAX_SIZE + 1;
        assert!(installer(&r).is_err());
    }
    #[test]
    fn refuses_tampered_or_truncated_download() {
        let payload = b"fixture";
        let hash = Sha256::digest(payload);
        let expected = hex::encode(hash);
        assert!(verify(&hash, &expected, 7, 7).is_ok());
        assert!(verify(&Sha256::digest(b"changed"), &expected, 7, 7).is_err());
        assert!(verify(&hash, &expected, 6, 7).is_err());
    }
}
