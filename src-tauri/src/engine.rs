use crate::{
    media,
    providers::{self, Reply, Task},
    subtitles,
    types::*,
};
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct Control {
    pub pause: AtomicBool,
    pub cancel: AtomicBool,
}
impl Control {
    pub fn canceled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub async fn until_canceled(&self) {
        while !self.canceled() {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    pub async fn checkpoint(&self) -> Result<()> {
        while self.pause.load(Ordering::SeqCst) {
            ensure!(!self.canceled(), "canceled");
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        ensure!(!self.canceled(), "canceled");
        Ok(())
    }
    pub async fn sleep(&self, duration: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + duration;
        while tokio::time::Instant::now() < deadline {
            self.checkpoint().await?;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(())
    }
}
pub struct Rate {
    pub next: tokio::time::Instant,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ModelStatus {
    pub id: String,
    pub installed: bool,
    pub downloading: bool,
    pub downloaded: u64,
    pub total: u64,
    pub message: String,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub settings: Settings,
    pub jobs: Vec<Job>,
    pub models: Vec<ModelStatus>,
    pub keys: HashMap<String, bool>,
    pub running: bool,
    pub version: String,
    pub cli: HashMap<String, bool>,
}
pub struct Engine {
    pub app: tauri::AppHandle,
    pub dir: PathBuf,
    pub resources: PathBuf,
    pub settings: Mutex<Settings>,
    pub jobs: Mutex<Vec<Job>>,
    pub controls: Mutex<HashMap<String, Arc<Control>>>,
    pub active: Mutex<std::collections::HashSet<String>>,
    pub rates: Mutex<HashMap<String, Arc<tokio::sync::Mutex<Rate>>>>,
    pub local_lock: tokio::sync::Mutex<()>,
    pub running: AtomicBool,
    pub downloads: Mutex<HashMap<String, ModelStatus>>,
    pub logins: Mutex<HashMap<String, crate::agents::Login>>,
    save_lock: Mutex<()>,
    log_lock: Mutex<()>,
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path)?;
    Ok(())
}
fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
fn log_path(dir: &Path, id: &str) -> Result<PathBuf> {
    ensure!(uuid::Uuid::parse_str(id).is_ok(), "invalid_job_id");
    Ok(dir.join("logs").join(format!("{id}.jsonl")))
}
fn append_log(dir: &Path, id: &str, entry: &serde_json::Value) -> Result<()> {
    use std::io::Write;
    let path = log_path(dir, id)?;
    fs::create_dir_all(path.parent().unwrap())?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", serde_json::to_string(entry)?)?;
    Ok(())
}
fn scrub(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            out = out.replace(secret, "[redacted]");
        }
    }
    if let Ok(urls) = regex::Regex::new(r"https?://[^\s]+") {
        out = urls.replace_all(&out, "[URL omitted]").into_owned();
    }
    out.chars().take(800).collect()
}
fn log_error_text(text: &str) -> String {
    regex::Regex::new(r"(?s)(process_failed|cli_failed):.*")
        .unwrap()
        .replace_all(text, "$1: raw process output omitted")
        .into_owned()
}
fn cached_transcript(path: &Path, duration: f64, empty_ok: bool) -> Option<Transcript> {
    let t: Transcript = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if t.cues.is_empty() && empty_ok || subtitles::validate(&t.cues, duration).is_ok() {
        Some(t)
    } else {
        None
    }
}
fn valid_translation_checkpoint(cues: &[Cue], source: &[Cue]) -> bool {
    cues.len() == source.len()
        && subtitles::validate(cues, 0.).is_ok()
        && cues
            .iter()
            .zip(source)
            .all(|(a, b)| a.id == b.id && a.start == b.start && a.end == b.end)
}
impl Engine {
    pub fn new(app: tauri::AppHandle) -> Result<Arc<Self>> {
        let dir = std::env::var_os("SUBFORGE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or(app.path().app_data_dir()?);
        fs::create_dir_all(dir.join("models"))?;
        fs::create_dir_all(dir.join("cache"))?;
        fs::create_dir_all(dir.join("logs"))?;
        let mut settings: Settings = fs::read(dir.join("settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        for p in settings
            .translation
            .iter_mut()
            .filter(|p| p.kind == "antigravity" && p.id == "antigravity")
        {
            if p.models.iter().all(|m| m.trim().is_empty()) {
                p.models = vec!["gemini-3.8-flash-low".into()];
                if !providers::has_key(&p.id) && p.auth == "key" {
                    p.auth = "oauth".into();
                }
                if p.cost == "unknown" {
                    p.cost = "free".into();
                }
            }
        }
        let mut jobs: Vec<Job> = fs::read(dir.join("jobs.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        for j in &mut jobs {
            if !matches!(
                j.status.as_str(),
                "completed" | "skipped" | "failed" | "canceled"
            ) {
                j.status = "paused".into();
                j.message = "resume_available".into();
            }
        }
        let engine = Arc::new(Self {
            resources: app.path().resource_dir()?,
            app,
            dir,
            settings: Mutex::new(settings),
            jobs: Mutex::new(jobs),
            controls: Mutex::new(HashMap::new()),
            active: Mutex::new(std::collections::HashSet::new()),
            rates: Mutex::new(HashMap::new()),
            local_lock: tokio::sync::Mutex::new(()),
            running: AtomicBool::new(false),
            downloads: Mutex::new(HashMap::new()),
            logins: Mutex::new(HashMap::new()),
            save_lock: Mutex::new(()),
            log_lock: Mutex::new(()),
        });
        engine.cleanup();
        Ok(engine)
    }
    fn cleanup(&self) {
        let days = self.settings.lock().unwrap().retention_days as u64;
        if days == 0
            || self
                .jobs
                .lock()
                .unwrap()
                .iter()
                .any(|j| !matches!(j.status.as_str(), "completed" | "skipped" | "canceled"))
        {
            return;
        }
        if let Ok(entries) = fs::read_dir(self.dir.join("cache")) {
            for entry in entries.flatten() {
                if let Ok(age) = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .and_then(|t| t.elapsed().map_err(std::io::Error::other))
                {
                    if age.as_secs() > days * 86400 {
                        let _ = fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
    }
    pub fn binary(&self, name: &str) -> PathBuf {
        let installed = self.resources.join("bin").join(name);
        if installed.exists() {
            installed
        } else {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/bin")
                .join(name)
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        let settings = self.settings.lock().unwrap().clone();
        let keys = settings
            .speech
            .iter()
            .chain(&settings.translation)
            .map(|p| (p.id.clone(), providers::has_key(&p.id)))
            .collect();
        let models = [("base", 147951465), ("small", 487601967)]
            .iter()
            .map(|(id, size)| {
                self.downloads
                    .lock()
                    .unwrap()
                    .get(*id)
                    .cloned()
                    .unwrap_or(ModelStatus {
                        id: id.to_string(),
                        installed: self
                            .dir
                            .join("models")
                            .join(format!("ggml-{id}.bin"))
                            .metadata()
                            .map(|m| m.len() == *size)
                            .unwrap_or(false),
                        downloading: false,
                        downloaded: 0,
                        total: *size,
                        message: String::new(),
                    })
            })
            .collect();
        let cli = settings
            .translation
            .iter()
            .filter(|p| matches!(p.kind.as_str(), "zen" | "opencode" | "pi" | "antigravity"))
            .map(|p| (p.id.clone(), crate::agents::ready(&self.dir, &p.kind)))
            .collect();
        Snapshot {
            settings,
            jobs: self.jobs.lock().unwrap().clone(),
            models,
            keys,
            running: self.running.load(Ordering::SeqCst),
            version: env!("CARGO_PKG_VERSION").into(),
            cli,
        }
    }
    pub fn emit(&self) {
        let _ = self.app.emit("changed", ());
    }
    pub fn save(&self) {
        let _guard = self.save_lock.lock().unwrap();
        let settings = self.settings.lock().unwrap().clone();
        let jobs = self.jobs.lock().unwrap().clone();
        let _ = atomic(
            &self.dir.join("settings.json"),
            &serde_json::to_vec_pretty(&settings).unwrap(),
        );
        let _ = atomic(
            &self.dir.join("jobs.json"),
            &serde_json::to_vec_pretty(&jobs).unwrap(),
        );
    }
    pub fn update(
        &self,
        id: &str,
        stage: &str,
        progress: Option<f64>,
        provider: &str,
        message: &str,
    ) {
        let message = self.redact(message);
        if let Some(j) = self.jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
            j.stage = stage.into();
            j.progress = progress;
            if !provider.is_empty() {
                j.provider = provider.into();
            }
            if !message.is_empty() {
                j.message = message.clone();
            }
            if j.status != "paused" {
                j.status = "running".into();
            }
        }
        self.event(id, "stage", serde_json::json!({"stage":stage,"progress":progress,"provider":provider,"message":message}));
        self.save();
        self.emit();
    }
    pub fn event(&self, id: &str, event: &str, data: serde_json::Value) {
        let (attempt, stage) = match self.jobs.lock().unwrap().iter().find(|j| j.id == id) {
            Some(j) => (j.attempt, j.stage.clone()),
            None => return, // Connection tests are not file jobs.
        };
        fn clean(secrets: &[String], value: serde_json::Value) -> serde_json::Value {
            match value {
                serde_json::Value::String(s) => serde_json::Value::String(scrub(&s, secrets)),
                serde_json::Value::Array(a) => a.into_iter().map(|v| clean(secrets, v)).collect(),
                serde_json::Value::Object(o) => o
                    .into_iter()
                    .map(|(k, v)| {
                        let v = if matches!(k.as_str(), "error" | "message") {
                            if let Some(text) = v.as_str() {
                                serde_json::Value::String(log_error_text(text))
                            } else {
                                v
                            }
                        } else {
                            v
                        };
                        (k, clean(secrets, v))
                    })
                    .collect(),
                v => v,
            }
        }
        let entry = serde_json::json!({"time_unix":now_seconds(),"version":env!("CARGO_PKG_VERSION"),"job":id,"attempt":attempt,"stage":stage,"event":event,"data":clean(&self.secrets(),data)});
        let _guard = self.log_lock.lock().unwrap();
        let _ = append_log(&self.dir, id, &entry);
    }
    pub fn note(&self, id: &str, key: &str, args: serde_json::Value) {
        let mut args = args;
        if let Some(j) = self.jobs.lock().unwrap().iter_mut().find(|j| j.id == id) {
            if matches!(
                key,
                "waiting_local"
                    | "checking_model"
                    | "recognizing"
                    | "timing_repaired"
                    | "timing_clipped"
                    | "provider_attempt"
                    | "retry_wait"
                    | "cooldown_wait"
                    | "batch_split"
                    | "agent_request"
            ) {
                if let Some(map) = args.as_object_mut() {
                    for name in ["part", "total", "from", "to", "cues", "attempt", "attempts"] {
                        if !map.contains_key(name) {
                            if let Some(v) = j.activity["args"].get(name) {
                                map.insert(name.into(), v.clone());
                            }
                        }
                    }
                }
            }
            j.activity = serde_json::json!({"key":key,"args":args});
        }
        self.event(id, key, args);
        self.emit();
    }
    pub fn job_log(&self, id: &str) -> Result<String> {
        let _guard = self.log_lock.lock().unwrap();
        let path = log_path(&self.dir, id)?;
        if !path.exists() {
            return Ok(
                "No detailed log was recorded by this app version. Retry the file to record one."
                    .into(),
            );
        }
        Ok(fs::read_to_string(path)?)
    }
    pub fn redact(&self, text: &str) -> String {
        scrub(text, &self.secrets())
    }
    fn secrets(&self) -> Vec<String> {
        let settings = self.settings.lock().unwrap().clone();
        let mut ids: std::collections::HashSet<String> = settings
            .speech
            .iter()
            .chain(&settings.translation)
            .map(|p| p.id.clone())
            .collect();
        for j in self.jobs.lock().unwrap().iter() {
            ids.extend(
                j.config
                    .speech
                    .iter()
                    .chain(&j.config.translation)
                    .map(|p| p.id.clone()),
            );
        }
        ids.into_iter()
            .filter_map(|id| providers::key(&id).ok())
            .collect()
    }
    pub fn gate(&self, id: &str) -> Arc<tokio::sync::Mutex<Rate>> {
        self.rates
            .lock()
            .unwrap()
            .entry(id.into())
            .or_insert_with(|| {
                Arc::new(tokio::sync::Mutex::new(Rate {
                    next: tokio::time::Instant::now(),
                }))
            })
            .clone()
    }
    pub fn add(&self, paths: Vec<String>) -> Result<usize> {
        let settings = self.settings.lock().unwrap().clone();
        let mut files = Vec::new();
        for p in paths {
            collect_files(&PathBuf::from(p), settings.recursive, &mut files)?;
        }
        let mut jobs = self.jobs.lock().unwrap();
        let mut count = 0;
        for path in files {
            let path = fs::canonicalize(path)?;
            let value = path.to_string_lossy().to_string();
            if jobs.iter().any(|j| {
                j.path == value
                    && !matches!(
                        j.status.as_str(),
                        "completed" | "skipped" | "canceled" | "failed"
                    )
            }) {
                continue;
            }
            jobs.push(Job::new(value, settings.clone()));
            count += 1;
        }
        drop(jobs);
        let added = self.jobs.lock().unwrap().clone();
        for j in added.iter().rev().take(count) {
            self.event(
                &j.id,
                "queued",
                serde_json::json!({"file":j.path,"target":j.target}),
            );
        }
        self.save();
        self.emit();
        Ok(count)
    }
    pub fn action(&self, id: &str, action: &str) -> Result<()> {
        let retry_settings = if action == "retry" {
            Some(self.settings.lock().unwrap().clone())
        } else {
            None
        };
        let active = self.active.lock().unwrap().contains(id);
        if action == "retry" {
            ensure!(!active, "job_stopping");
        }
        let control = self
            .controls
            .lock()
            .unwrap()
            .entry(id.into())
            .or_default()
            .clone();
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs
            .iter_mut()
            .find(|j| j.id == id)
            .ok_or_else(|| anyhow::anyhow!("job_missing"))?;
        if let Some(config) = retry_settings {
            job.target = config.target_language.clone();
            job.config = config;
        }
        match action {
            "pause" => {
                control.pause.store(true, Ordering::SeqCst);
                job.status = "paused".into();
                job.message = "pause_boundary".into();
            }
            "resume" | "retry" => {
                if control.canceled() && active {
                    bail!("job_stopping");
                }
                control.pause.store(false, Ordering::SeqCst);
                control.cancel.store(false, Ordering::SeqCst);
                job.status = if active { "running" } else { "queued" }.into();
                if !active {
                    job.stage = "queued".into();
                    job.message.clear();
                    job.provider.clear();
                    job.progress = None;
                    job.activity = serde_json::json!({"key":"retry_queued","args":{}});
                }
                self.running.store(true, Ordering::SeqCst);
            }
            "cancel" => {
                control.cancel.store(true, Ordering::SeqCst);
                control.pause.store(false, Ordering::SeqCst);
                job.status = "canceled".into();
            }
            _ => bail!("invalid_action"),
        };
        drop(jobs);
        self.event(id, action, serde_json::json!({}));
        self.save();
        self.emit();
        Ok(())
    }
    pub fn scheduler(self: &Arc<Self>) {
        let engine = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                if engine.running.load(Ordering::SeqCst) {
                    let limit = engine.settings.lock().unwrap().parallel_files;
                    let available = limit.saturating_sub(engine.active.lock().unwrap().len());
                    let pending: Vec<_> = engine
                        .jobs
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|j| j.status == "queued")
                        .filter(|j| !engine.active.lock().unwrap().contains(&j.id))
                        .take(available)
                        .cloned()
                        .collect();
                    for job in pending {
                        engine.active.lock().unwrap().insert(job.id.clone());
                        if let Some(j) = engine
                            .jobs
                            .lock()
                            .unwrap()
                            .iter_mut()
                            .find(|j| j.id == job.id)
                        {
                            j.attempt += 1;
                            j.started_at = now_seconds();
                        }
                        engine.event(&job.id, "started", serde_json::json!({"file":job.path,"target":job.target,"transcription":job.config.transcription_mode,"local_model":job.config.local_model,"chunk_seconds":job.config.chunk_seconds,"batch_size":job.config.batch_size,"cloud_fallback":job.config.cloud_fallback,"translation_routes":job.config.translation.iter().map(|p|serde_json::json!({"name":p.name,"kind":p.kind,"models":p.models,"enabled":p.enabled,"retries":p.retries,"delay_ms":p.delay_ms,"timeout_secs":p.timeout_secs})).collect::<Vec<_>>()}));
                        let control = engine
                            .controls
                            .lock()
                            .unwrap()
                            .entry(job.id.clone())
                            .or_default()
                            .clone();
                        engine.update(&job.id, "inspecting", None, "", "");
                        let copy = engine.clone();
                        tauri::async_runtime::spawn(async move {
                            let result = copy.process(&job, &control).await;
                            let outcome = match &result {
                                Ok(output) => serde_json::json!({"output":output}),
                                Err(e) => serde_json::json!({"error":copy.redact(&e.to_string())}),
                            };
                            let mut outcome = outcome;
                            if let Some(j) =
                                copy.jobs.lock().unwrap().iter().find(|j| j.id == job.id)
                            {
                                outcome["last_stage"] = serde_json::json!(j.stage);
                                outcome["activity"] = j.activity.clone();
                                outcome["elapsed_seconds"] =
                                    serde_json::json!((now_seconds() - j.started_at).max(0.));
                            }
                            let mut jobs = copy.jobs.lock().unwrap();
                            if let Some(j) = jobs.iter_mut().find(|j| j.id == job.id) {
                                match result {
                                    Ok(output) => {
                                        if output.is_empty() {
                                            j.status = "skipped".into();
                                        } else {
                                            j.status = "completed".into();
                                            j.output = output;
                                        }
                                        j.stage = j.status.clone();
                                        j.progress = Some(1.);
                                        j.message = String::new();
                                    }
                                    Err(_) => {
                                        j.status = if control.canceled() {
                                            "canceled"
                                        } else {
                                            "failed"
                                        }
                                        .into();
                                        j.stage = j.status.clone();
                                        j.message = outcome["error"]
                                            .as_str()
                                            .unwrap_or("processing_failed")
                                            .into();
                                    }
                                }
                            }
                            drop(jobs);
                            copy.event(&job.id, "finished", outcome);
                            copy.active.lock().unwrap().remove(&job.id);
                            copy.save();
                            copy.emit();
                        });
                    }
                }
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        });
    }
    async fn process(self: &Arc<Self>, job: &Job, control: &Control) -> Result<String> {
        let settings = &job.config;
        settings.validate()?;
        let input = Path::new(&job.path);
        let metadata = fs::metadata(input)?;
        let mut hasher = Sha256::new();
        hasher.update(job.path.as_bytes());
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(format!("{:?}", metadata.modified()?).as_bytes());
        hasher.update(serde_json::to_vec(&serde_json::json!({"target":settings.target_language,"source":settings.source_language,"track":settings.audio_track,"mode":settings.transcription_mode,"model":settings.local_model,"chunk":settings.chunk_seconds,"subtitle":settings.subtitle_override,"folder":settings.subtitle_folder}))?);
        // External subtitle changes invalidate transcription and translation checkpoints.
        let parent = input.parent().unwrap_or(Path::new("."));
        for dir in [
            parent.to_path_buf(),
            parent.join("Subs"),
            parent.join("Subtitles"),
            parent.join(input.file_stem().unwrap_or_default()),
            PathBuf::from(&settings.subtitle_folder),
        ] {
            if let Ok(entries) = fs::read_dir(dir) {
                let mut paths: Vec<_> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        matches!(
                            p.extension().and_then(|e| e.to_str()),
                            Some("srt" | "vtt" | "ass" | "ssa")
                        )
                    })
                    .collect();
                paths.sort();
                for p in paths {
                    let m = fs::metadata(&p)?;
                    hasher.update(p.to_string_lossy().as_bytes());
                    hasher.update(m.len().to_le_bytes());
                    hasher.update(format!("{:?}", m.modified()?).as_bytes());
                }
            }
        }
        if !settings.subtitle_override.is_empty() {
            hasher.update(fs::read(&settings.subtitle_override)?);
        }
        let cache = self.dir.join("cache").join(hex::encode(hasher.finalize()));
        fs::create_dir_all(&cache)?;
        control.checkpoint().await?;
        self.update(&job.id, "inspecting", None, "", "");
        let probe = media::probe(self, input, control).await?;
        self.note(
            &job.id,
            "video_checked",
            serde_json::json!({"seconds":probe.duration.round(),"streams":probe.streams}),
        );
        if let Some(j) = self
            .jobs
            .lock()
            .unwrap()
            .iter_mut()
            .find(|j| j.id == job.id)
        {
            j.duration = probe.duration;
        }
        self.update(&job.id, "finding_subtitles", None, "", "");
        let source_file = cache.join("source.json");
        let cached_source = cached_transcript(&source_file, probe.duration, false);
        let source_reused = cached_source.is_some();
        let mut transcript: Transcript = if let Some(t) = cached_source {
            self.note(
                &job.id,
                "source_reused",
                serde_json::json!({"cues":t.cues.len()}),
            );
            t
        } else {
            if source_file.exists() {
                self.note(
                    &job.id,
                    "cache_rebuilt",
                    serde_json::json!({"checkpoint":"source"}),
                );
            }
            self.note(&job.id, "searching_subtitles", serde_json::json!({}));
            let candidates = media::discover(input, settings, probe.duration)?;
            let existing = if let Some(first) = candidates.first() {
                if candidates
                    .get(1)
                    .is_some_and(|c| c.score == first.score && c.language != first.language)
                {
                    bail!("subtitle_ambiguous");
                }
                Some((
                    Transcript {
                        language: first.language.clone(),
                        cues: subtitles::read(&first.path)?,
                    },
                    first.path.to_string_lossy().to_string(),
                ))
            } else {
                media::embedded(self, input, &probe, settings, &cache, control).await?
            };
            if let Some((t, source)) = existing {
                self.note(
                    &job.id,
                    "subtitle_found",
                    serde_json::json!({"file":source,"cues":t.cues.len(),"language":t.language}),
                );
                self.update(
                    &job.id,
                    "finding_subtitles",
                    None,
                    "",
                    &format!("subtitle_source:{source}"),
                );
                if t.language == settings.target_language {
                    return Ok(String::new());
                }
                t
            } else {
                let index = media::audio_track(&probe, settings)?;
                self.event(
                    &job.id,
                    "audio_selected",
                    serde_json::json!({"track":index}),
                );
                let detected = probe
                    .streams
                    .iter()
                    .find(|s| s.index == index)
                    .map(|s| s.language.clone())
                    .unwrap_or("auto".into());
                let source = if settings.source_language != "auto" {
                    settings.source_language.clone()
                } else {
                    detected
                };
                let chunk = settings.chunk_seconds.min(300) as f64;
                let step = (chunk - 2.).max(1.);
                let count = (probe.duration / step).ceil() as usize;
                let mut chunks = Vec::new();
                for i in 0..count {
                    control.checkpoint().await?;
                    let offset = i as f64 * step;
                    let length = chunk.min(probe.duration - offset);
                    if length <= 0.1 {
                        continue;
                    }
                    let part = cache.join(format!("chunk-{i}"));
                    fs::create_dir_all(&part)?;
                    let saved = part.join("transcript.json");
                    if let Some(t) = cached_transcript(&saved, length, true) {
                        self.note(
                            &job.id,
                            "chunk_reused",
                            serde_json::json!({"part":i+1,"total":count,"cues":t.cues.len()}),
                        );
                        chunks.push((offset, t));
                        continue;
                    }
                    if saved.exists() {
                        self.note(
                            &job.id,
                            "cache_rebuilt",
                            serde_json::json!({"checkpoint":format!("chunk-{i}")}),
                        );
                    }
                    let audio = part.join("audio.wav");
                    self.update(&job.id, "extracting", Some(i as f64 / count as f64), "", "");
                    self.note(&job.id,"audio_part",serde_json::json!({"part":i+1,"total":count,"from":offset.round(),"to":(offset+length).round()}));
                    if !audio.exists() {
                        media::extract(self, input, index, offset, length, &audio, control).await?;
                    }
                    let mut result = if media::silent_wav(&audio)? {
                        self.note(
                            &job.id,
                            "silence",
                            serde_json::json!({"part":i+1,"total":count}),
                        );
                        Ok(Transcript {
                            language: source.clone(),
                            cues: Vec::new(),
                        })
                    } else if settings.transcription_mode == "local" {
                        self.update(
                            &job.id,
                            "transcribing",
                            Some(i as f64 / count as f64),
                            &format!("Whisper {} (CPU)", settings.local_model),
                            "",
                        );
                        self.note(&job.id,"speech_part",serde_json::json!({"part":i+1,"total":count,"from":offset.round(),"to":(offset+length).round()}));
                        media::local(
                            self,
                            &audio,
                            &settings.local_model,
                            &source,
                            &part,
                            control,
                            &job.id,
                            length,
                        )
                        .await
                    } else {
                        match providers::route(
                            self,
                            &job.id,
                            settings,
                            &settings.speech,
                            Task::Speech {
                                audio: audio.clone(),
                                source: source.clone(),
                                dir: part.clone(),
                            },
                            control,
                        )
                        .await?
                        {
                            Reply::Transcribed(t) => Ok(t),
                            _ => bail!("invalid_response"),
                        }
                    };
                    if result.is_err()
                        && settings.transcription_mode == "local"
                        && settings.cloud_fallback
                    {
                        self.event(&job.id,"local_failed_cloud_fallback",serde_json::json!({"error":result.as_ref().err().map(|e|e.to_string())}));
                        result = match providers::route(
                            self,
                            &job.id,
                            settings,
                            &settings.speech,
                            Task::Speech {
                                audio: audio.clone(),
                                source: source.clone(),
                                dir: part.clone(),
                            },
                            control,
                        )
                        .await?
                        {
                            Reply::Transcribed(t) => Ok(t),
                            _ => bail!("invalid_response"),
                        };
                    }
                    let t = result?;
                    if !t.cues.is_empty() {
                        subtitles::validate(&t.cues, length)?;
                    }
                    atomic(&saved, &serde_json::to_vec(&t)?)?;
                    self.note(&job.id,"chunk_done",serde_json::json!({"part":i+1,"total":count,"cues":t.cues.len(),"language":t.language}));
                    let _ = fs::remove_file(&audio);
                    chunks.push((offset, t));
                    self.update(
                        &job.id,
                        "transcribing",
                        Some((i + 1) as f64 / count as f64),
                        "",
                        "",
                    );
                }
                subtitles::merge(chunks)
            }
        };
        subtitles::validate(&transcript.cues, probe.duration)?;
        self.note(
            &job.id,
            "transcript_ready",
            serde_json::json!({"cues":transcript.cues.len(),"language":transcript.language}),
        );
        if !source_reused {
            atomic(&source_file, &serde_json::to_vec(&transcript)?)?;
        }
        if transcript.language != settings.target_language {
            let translation_key = hex::encode(Sha256::digest(serde_json::to_vec(
                &serde_json::json!({"routes":settings.translation,"batch":settings.batch_size,"target":settings.target_language,"protocol":2}),
            )?));
            let original = transcript.cues.clone();
            let mut translated = Vec::new();
            let batches = subtitles::translation_batches(&original, settings.batch_size);
            let count = batches.len();
            for (i, range) in batches.into_iter().enumerate() {
                let batch = &original[range.clone()];
                control.checkpoint().await?;
                let checkpoint = cache.join(format!("translated-{translation_key}-{i}.json"));
                self.update(
                    &job.id,
                    "translating",
                    Some(i as f64 / count as f64),
                    "",
                    "",
                );
                self.note(&job.id,"translation_part",serde_json::json!({"part":i+1,"total":count,"cues":batch.len(),"first":batch.first().map(|c|c.id),"last":batch.last().map(|c|c.id)}));
                let cached: Option<Vec<Cue>> = fs::read(&checkpoint)
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .filter(|c: &Vec<Cue>| valid_translation_checkpoint(c, batch));
                let result: Vec<Cue> = if let Some(c) = cached {
                    self.event(
                        &job.id,
                        "translation_reused",
                        serde_json::json!({"part":i+1,"cues":c.len()}),
                    );
                    c
                } else {
                    if checkpoint.exists() {
                        self.event(
                            &job.id,
                            "cache_rebuilt",
                            serde_json::json!({"checkpoint":format!("translation-{i}")}),
                        );
                    }
                    let start = range.start;
                    let context = original[start.saturating_sub(4)..start].to_vec();
                    match providers::route(
                        self,
                        &job.id,
                        settings,
                        &settings.translation,
                        Task::Translate {
                            cues: batch.to_vec(),
                            context,
                            source: transcript.language.clone(),
                            target: settings.target_language.clone(),
                        },
                        control,
                    )
                    .await?
                    {
                        Reply::Translated(c) => c,
                        _ => bail!("invalid_response"),
                    }
                };
                ensure!(
                    result.len() == batch.len()
                        && result
                            .iter()
                            .zip(batch)
                            .all(|(a, b)| a.id == b.id && a.start == b.start && a.end == b.end),
                    "invalid_checkpoint"
                );
                atomic(&checkpoint, &serde_json::to_vec(&result)?)?;
                self.event(
                    &job.id,
                    "translation_saved",
                    serde_json::json!({"part":i+1,"cues":result.len()}),
                );
                translated.extend(result);
                self.update(
                    &job.id,
                    "translating",
                    Some((i + 1) as f64 / count as f64),
                    "",
                    "",
                );
            }
            transcript.cues = translated;
        }
        control.checkpoint().await?;
        self.update(&job.id, "validating", None, "", "");
        self.note(
            &job.id,
            "checking_output",
            serde_json::json!({"cues":transcript.cues.len()}),
        );
        subtitles::validate(&transcript.cues, probe.duration)?;
        let parent = if settings.output_directory.is_empty() {
            input.parent().unwrap().to_path_buf()
        } else {
            PathBuf::from(&settings.output_directory)
        };
        fs::create_dir_all(&parent)?;
        let stem = input.file_stem().unwrap_or_default().to_string_lossy();
        let name = settings
            .filename_pattern
            .replace("{name}", &stem)
            .replace("{language}", &settings.target_language);
        ensure!(
            !name.is_empty() && !name.contains(['/', '\\']),
            "invalid_filename"
        );
        self.update(&job.id, "saving", None, "", "");
        let text = subtitles::render(&transcript.cues, &settings.output_format);
        let output = if settings.overwrite {
            let output = output_path(&parent, &name, &settings.output_format, true);
            atomic(&output, text.as_bytes())?;
            output
        } else {
            safe_output(&parent, &name, &settings.output_format, text.as_bytes())?
        };
        if settings.retain_transcript {
            let source: Transcript = serde_json::from_slice(&fs::read(&source_file)?)?;
            safe_output(
                &parent,
                &format!("{stem}.source"),
                "srt",
                subtitles::render(&source.cues, "srt").as_bytes(),
            )?;
        }
        Ok(output.to_string_lossy().into_owned())
    }
}
fn safe_output(parent: &Path, name: &str, format: &str, bytes: &[u8]) -> Result<PathBuf> {
    use std::io::Write;
    let temporary = parent.join(format!(".subforge-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        for i in 1.. {
            let output = parent.join(if i == 1 {
                format!("{name}.{format}")
            } else {
                format!("{name}.{i}.{format}")
            });
            match fs::hard_link(&temporary, &output) {
                Ok(()) => return Ok(output),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                // FAT/exFAT volumes do not support links. Rename publishes the complete
                // temp file without clobbering on Windows (MoveFileW).
                Err(_) => match rename_new(&temporary, &output) {
                    Ok(()) => return Ok(output),
                    Err(_) if output.exists() => continue,
                    Err(e) => return Err(e.into()),
                },
            }
        }
        unreachable!()
    })();
    let _ = fs::remove_file(&temporary);
    result
}
fn rename_new(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileW(source: *const u16, destination: *const u16) -> i32;
        }
        let source: Vec<_> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<_> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe { MoveFileW(source.as_ptr(), destination.as_ptr()) } == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)
    }
}
pub fn file_hash(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut reader = std::io::BufReader::new(fs::File::open(path)?);
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = reader.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(hex::encode(hash.finalize()))
}
fn output_path(parent: &Path, name: &str, format: &str, overwrite: bool) -> PathBuf {
    let path = parent.join(format!("{name}.{format}"));
    if overwrite || !path.exists() {
        return path;
    }
    for i in 2.. {
        let path = parent.join(format!("{name}.{i}.{format}"));
        if !path.exists() {
            return path;
        }
    }
    unreachable!()
}
fn collect_files(path: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_dir() {
        for e in fs::read_dir(path)?.flatten() {
            if e.file_type()?.is_symlink() {
                continue;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if e.metadata()?.file_attributes() & 0x400 != 0 {
                    continue;
                }
            }
            let p = e.path();
            if p.is_file() || recursive {
                collect_files(&p, recursive, out)?;
            }
        }
    } else {
        ensure!(path.exists(), "file_missing");
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if matches!(
            ext.as_str(),
            "mkv"
                | "mp4"
                | "avi"
                | "mov"
                | "webm"
                | "m4v"
                | "wmv"
                | "mpg"
                | "mpeg"
                | "ts"
                | "mts"
                | "m2ts"
                | "flv"
                | "ogv"
                | "vob"
                | "3gp"
        ) {
            out.push(path.to_path_buf());
        }
    }
    Ok(())
}
pub const MODEL_DATA: [(&str, u64, &str); 2] = [
    (
        "base",
        147951465,
        "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    ),
    (
        "small",
        487601967,
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
    ),
];
pub async fn download(engine: Arc<Engine>, id: String) -> Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let (_, size, hash) = MODEL_DATA
        .iter()
        .find(|(name, _, _)| *name == id)
        .ok_or_else(|| anyhow::anyhow!("invalid_model"))?;
    let mut status = ModelStatus {
        id: id.clone(),
        installed: false,
        downloading: true,
        downloaded: 0,
        total: *size,
        message: String::new(),
    };
    {
        let mut downloads = engine.downloads.lock().unwrap();
        ensure!(
            !downloads.get(&id).is_some_and(|s| s.downloading),
            "model_downloading"
        );
        downloads.insert(id.clone(), status.clone());
    }
    engine.emit();
    let part = engine
        .dir
        .join("models")
        .join(format!("ggml-{id}.bin.part"));
    let result:Result<()>=async{let offset=fs::metadata(&part).map(|m|m.len()).unwrap_or(0);let url=format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-{id}.bin");let c=reqwest::Client::builder().timeout(Duration::from_secs(3600)).build()?;let response=c.get(url).header("Range",format!("bytes={offset}-")).send().await?;let append=offset>0&&response.status()==reqwest::StatusCode::PARTIAL_CONTENT;if response.status()==reqwest::StatusCode::RANGE_NOT_SATISFIABLE&&offset==*size{}else{ensure!(response.status().is_success(),"model_download_failed");status.downloaded=if append{offset}else{0};let mut file=tokio::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(&part).await?;let mut stream=response.bytes_stream();let mut last=tokio::time::Instant::now();while let Some(chunk)=stream.next().await{let bytes=chunk?;file.write_all(&bytes).await?;status.downloaded+=bytes.len()as u64;if last.elapsed()>Duration::from_millis(250){engine.downloads.lock().unwrap().insert(id.clone(),status.clone());engine.emit();last=tokio::time::Instant::now();}}file.flush().await?;}
 if fs::metadata(&part)?.len()!=*size || file_hash(&part)?!=*hash { let _=fs::remove_file(&part); bail!("model_checksum_failed"); }fs::rename(&part,engine.dir.join("models").join(format!("ggml-{id}.bin")))?;Ok(())}.await;
    status.downloading = false;
    status.installed = result.is_ok();
    status.downloaded = if status.installed {
        *size
    } else {
        status.downloaded
    };
    status.message = result
        .as_ref()
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    engine.downloads.lock().unwrap().insert(id, status);
    engine.emit();
    result
}
#[cfg(test)]
mod tests {
    #[test]
    fn invalid_transcript_cache_is_rebuilt_and_valid_partial_work_is_reused() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("transcript.json");
        let mut t = Transcript {
            language: "en".into(),
            cues: vec![Cue {
                id: 1,
                start: 1.,
                end: 1.,
                text: "Synthetic".into(),
            }],
        };
        atomic(&path, &serde_json::to_vec(&t).unwrap()).unwrap();
        assert!(cached_transcript(&path, 10., false).is_none());
        t.cues[0].end = 2.;
        atomic(&path, &serde_json::to_vec(&t).unwrap()).unwrap();
        assert!(cached_transcript(&path, 10., false).is_some());
        t.cues.clear();
        atomic(&path, &serde_json::to_vec(&t).unwrap()).unwrap();
        assert!(cached_transcript(&path, 10., true).is_some());
        assert!(cached_transcript(&path, 10., false).is_none());
        fs::write(&path, "{\"cues\":").unwrap();
        assert!(cached_transcript(&path, 10., true).is_none());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn translation_checkpoint_cannot_reuse_invalid_or_changed_cues() {
        let c = vec![Cue {
            id: 1,
            start: 1.,
            end: 2.,
            text: "Synthetic".into(),
        }];
        assert!(valid_translation_checkpoint(&c, &c));
        let mut invalid = c.clone();
        invalid[0].text.clear();
        assert!(!valid_translation_checkpoint(&invalid, &c));
        invalid = c.clone();
        invalid[0].end = 3.;
        assert!(!valid_translation_checkpoint(&invalid, &c));
    }
    #[test]
    fn file_logs_append_attempts_and_cannot_escape_the_log_folder() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let id = uuid::Uuid::new_v4().to_string();
        append_log(
            &dir,
            &id,
            &serde_json::json!({"attempt":1,"file":"quotes\" and\nnewlines"}),
        )
        .unwrap();
        append_log(&dir, &id, &serde_json::json!({"attempt":2,"event":"retry"})).unwrap();
        let text = fs::read_to_string(log_path(&dir, &id).unwrap()).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1]["attempt"], 2);
        assert!(log_path(&dir, "../settings").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn logs_redact_secrets_and_authorization_urls() {
        let text = scrub(
            "http_403: https://accounts.google.com/login?token=secret secret",
            &["secret".into()],
        );
        assert!(!text.contains("secret"));
        assert!(!text.contains("https://"));
        assert!(text.contains("http_403"));
        assert!(!log_error_text(
            "process_failed: prefix | synthetic dialogue printed by a child\nmore output"
        )
        .contains("synthetic dialogue"));
    }
    use super::*;
    #[test]
    fn unique_output_preserves_existing() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Movie.el.srt"), "original").unwrap();
        assert!(output_path(&dir, "Movie.el", "srt", false).ends_with("Movie.el.2.srt"));
        assert_eq!(
            fs::read_to_string(dir.join("Movie.el.srt")).unwrap(),
            "original"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn concurrent_outputs_are_complete_and_never_clobber() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Movie.el.srt"), "original").unwrap();
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    safe_output(&dir, "Movie.el", "srt", format!("complete-{i}").as_bytes())
                        .unwrap()
                })
            })
            .collect();
        let paths: std::collections::HashSet<_> =
            handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(paths.len(), 8);
        for p in paths {
            assert!(fs::read_to_string(p).unwrap().starts_with("complete-"));
        }
        assert_eq!(
            fs::read_to_string(dir.join("Movie.el.srt")).unwrap(),
            "original"
        );
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 9);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn rename_fallback_refuses_existing_destination() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("new.tmp");
        let destination = dir.join("existing.srt");
        fs::write(&source, "new").unwrap();
        fs::write(&destination, "original").unwrap();
        assert!(rename_new(&source, &destination).is_err());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "original");
        assert!(source.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
