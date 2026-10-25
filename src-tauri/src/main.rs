#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod agents;
mod engine;
mod media;
mod openai_oauth;
mod pacing;
mod providers;
mod subtitles;
mod types;
mod updates;
use engine::Engine;
use std::{
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
};
use tauri::{Manager, State};
use types::{Provider, Settings};
type AppState = Arc<Engine>;
type CmdResult<T> = Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
fn snapshot(state: State<AppState>) -> engine::Snapshot {
    state.snapshot()
}
#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> CmdResult<()> {
    settings.validate().map_err(error)?;
    engine::atomic(
        &state.dir.join("settings.json"),
        &serde_json::to_vec_pretty(&settings).map_err(error)?,
    )
    .map_err(error)?;
    shell(settings.shell_enabled, &settings.ui_language).map_err(error)?;
    *state.settings.lock().unwrap() = settings;
    state.save();
    state.emit();
    Ok(())
}
#[tauri::command]
fn save_key(state: State<AppState>, id: String, value: String) -> CmdResult<()> {
    providers::set_key(&id, &value).map_err(error)?;
    state.emit();
    Ok(())
}
#[tauri::command]
fn key_saved(id: String) -> bool {
    providers::has_key(&id)
}
#[tauri::command]
fn add_files(state: State<AppState>, paths: Vec<String>) -> CmdResult<usize> {
    state.add(paths).map_err(error)
}
#[tauri::command]
fn start_queue(state: State<AppState>) {
    state.running.store(true, Ordering::SeqCst);
    let mut jobs = state.jobs.lock().unwrap();
    let config = state.settings.lock().unwrap().clone();
    for job in jobs.iter_mut().filter(|j| j.status == "queued") {
        job.target = config.target_language.clone();
        job.config = config.clone();
    }
    drop(jobs);
    state.save();
    state.emit();
}
#[tauri::command]
fn job_action(state: State<AppState>, id: String, action: String) -> CmdResult<()> {
    state.action(&id, &action).map_err(error)
}
#[tauri::command]
fn clear_finished(state: State<AppState>) {
    state.jobs.lock().unwrap().retain(|j| {
        !matches!(
            j.status.as_str(),
            "completed" | "skipped" | "failed" | "canceled"
        )
    });
    state.save();
    state.emit();
}
#[tauri::command]
async fn inspect_file(state: State<'_, AppState>, path: String) -> CmdResult<media::Probe> {
    media::probe(&state, &PathBuf::from(path), &engine::Control::default())
        .await
        .map_err(error)
}
#[tauri::command]
async fn download_model(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    engine::download(state.inner().clone(), id)
        .await
        .map_err(error)
}
#[tauri::command]
fn remove_model(state: State<AppState>, id: String) -> CmdResult<()> {
    if !matches!(id.as_str(), "base" | "small") {
        return Err("invalid_model".into());
    }
    if state.active.lock().unwrap().len() > 0
        || state
            .downloads
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|s| s.downloading)
    {
        return Err("model_in_use".into());
    }
    let path = state.dir.join("models").join(format!("ggml-{id}.bin"));
    if path.exists() {
        std::fs::remove_file(path).map_err(error)?;
    }
    state.downloads.lock().unwrap().remove(&id);
    state.emit();
    Ok(())
}
#[tauri::command]
async fn provider_models(state: State<'_, AppState>, provider: Provider) -> CmdResult<Vec<String>> {
    providers::models(&state, &provider, &engine::Control::default())
        .await
        .map_err(|e| providers::redact_provider(&e.to_string(), &provider))
}
#[tauri::command]
async fn test_provider(
    state: State<'_, AppState>,
    provider: Provider,
    role: String,
) -> CmdResult<String> {
    if role == "speech" {
        let list = providers::models(&state, &provider, &engine::Control::default())
            .await
            .map_err(error)?;
        if matches!(
            provider.kind.as_str(),
            "deepgram" | "speechmatics" | "cloudflare"
        ) {
            if !providers::has_key(&provider.id) {
                return Err("key_missing".into());
            }
            return Ok("key_saved_test_audio_required".into());
        }
        return Ok(format!("models_available:{}", list.len()));
    }
    let mut settings = state.settings.lock().unwrap().clone();
    let mut provider = provider;
    provider.enabled = true;
    if !types::eligible(&provider, &settings) {
        return Err("paid_disabled".into());
    }
    provider.retries = 0;
    settings.translation = vec![provider];
    let control = engine::Control::default();
    providers::route(
        state.inner(),
        "test",
        &settings,
        &settings.translation,
        providers::Task::Translate {
            part: 1,
            cues: vec![types::Cue {
                id: 1,
                start: 0.,
                end: 1.,
                text: "Hello.".into(),
            }],
            context: vec![],
            source: "en".into(),
            target: "el".into(),
        },
        &control,
    )
    .await
    .map_err(|e| state.redact(&e.to_string()))?;
    Ok("connection_ok".into())
}
#[tauri::command]
fn export_settings(state: State<AppState>, path: String) -> CmdResult<()> {
    engine::atomic(
        &PathBuf::from(path),
        &serde_json::to_vec_pretty(&*state.settings.lock().unwrap()).map_err(error)?,
    )
    .map_err(error)
}
#[tauri::command]
fn import_settings(state: State<AppState>, path: String) -> CmdResult<()> {
    let settings: Settings =
        serde_json::from_slice(&std::fs::read(path).map_err(error)?).map_err(error)?;
    save_settings(state, settings)
}
#[tauri::command]
fn open_output(state: State<AppState>, id: String) -> CmdResult<()> {
    let jobs = state.jobs.lock().unwrap();
    let job = jobs.iter().find(|j| j.id == id).ok_or("job_missing")?;
    let path = if job.output.is_empty() {
        PathBuf::from(&job.path)
    } else {
        PathBuf::from(&job.output)
    };
    std::process::Command::new("explorer.exe")
        .arg("/select,")
        .arg(path)
        .spawn()
        .map_err(error)?;
    Ok(())
}
#[tauri::command]
fn diagnostics(state: State<AppState>) -> String {
    let s = state.snapshot();
    format!("SubForge {}\nSidos - info@sidos.gr\nWindows x64\nData: {}\nFFmpeg: {}\nFFprobe: {}\nLocal models: {}\nConfigured translation routes: {}\nConfigured speech routes: {}\nCredentials are excluded.\nExplorer: classic menu (Windows 11: Show more options).",s.version,state.dir.display(),state.binary("ffmpeg.exe").exists(),state.binary("ffprobe.exe").exists(),s.models.iter().filter(|m|m.installed).map(|m|m.id.clone()).collect::<Vec<_>>().join(", "),s.settings.translation.len(),s.settings.speech.len())
}
#[tauri::command]
fn job_log(state: State<AppState>, id: String) -> CmdResult<String> {
    state.job_log(&id).map_err(error)
}
#[tauri::command]
fn export_job_log(state: State<AppState>, id: String, path: String) -> CmdResult<()> {
    engine::atomic(
        &PathBuf::from(path),
        state.job_log(&id).map_err(error)?.as_bytes(),
    )
    .map_err(error)
}
#[tauri::command]
fn open_provider_setup(url: String) -> CmdResult<()> {
    let allowed = [
        "https://opencode.ai/",
        "https://openrouter.ai/",
        "https://antigravity.google/",
        "https://pi.dev/",
        "https://console.groq.com/",
        "https://dash.cloudflare.com/",
        "https://aistudio.google.com/",
        "https://console.deepgram.com/",
        "https://portal.speechmatics.com/",
        "https://chatgpt.com/settings/usage",
    ];
    if !allowed.iter().any(|p| url.starts_with(p)) {
        return Err("setup_url_invalid".into());
    }
    agents::open_browser(&url).map_err(error)
}

#[tauri::command]
async fn prepare_agent(state: State<'_, AppState>, provider: Provider) -> CmdResult<String> {
    agents::prepare(&state, &provider.kind, &engine::Control::default())
        .await
        .map_err(error)?;
    Ok("agent_ready".into())
}
#[tauri::command]
async fn agent_signin(state: State<'_, AppState>, provider: Provider) -> CmdResult<String> {
    agents::prepare(&state, &provider.kind, &engine::Control::default())
        .await
        .map_err(error)?;
    agents::login(&state, &provider).map_err(error)?;
    Ok("signin_waiting".into())
}
#[tauri::command]
fn agent_signin_action(state: State<AppState>, id: String, action: String) -> CmdResult<String> {
    agents::login_action(&state, &id, &action).map_err(error)
}

#[tauri::command]
async fn openai_signin(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    openai_oauth::start(state.inner().clone(), id)
        .await
        .map_err(error)
}
#[tauri::command]
async fn openai_status(state: State<'_, AppState>, id: String) -> CmdResult<openai_oauth::Status> {
    openai_oauth::status(&state, &id).await.map_err(error)
}
#[tauri::command]
fn openai_cancel(state: State<AppState>, id: String) {
    openai_oauth::cancel(&state, &id);
}
#[tauri::command]
async fn openai_signout(state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    openai_oauth::signout(&state, &id).await.map_err(error)
}
#[tauri::command]
async fn openai_acknowledge(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    openai_oauth::acknowledge(&state, &id).await.map_err(error)
}
#[tauri::command]
async fn check_updates() -> CmdResult<updates::Available> {
    updates::check().await.map_err(error)
}
#[tauri::command]
fn update_status(state: State<AppState>) -> updates::Progress {
    updates::status(&state)
}
#[tauri::command]
async fn install_update(state: State<'_, AppState>, version: String) -> CmdResult<()> {
    updates::install(&state, &version).await.map_err(error)
}

fn shell(enabled: bool, language: &str) -> anyhow::Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let root = RegKey::predef(HKEY_CURRENT_USER);
    let exe = std::env::current_exe()?;
    let cmd = format!("\"{}\" --enqueue \"%1\"", exe.display());
    let title = match language {
        "el" => "Δημιουργία υποτίτλων με SubForge",
        "es" => "Crear subtítulos con SubForge",
        "fr" => "Créer des sous-titres avec SubForge",
        "de" => "Untertitel mit SubForge erstellen",
        "pt" => "Criar legendas com SubForge",
        "it" => "Crea sottotitoli con SubForge",
        "zh-Hans" => "使用 SubForge 创建字幕",
        "ko" => "SubForge로 자막 만들기",
        "ja" => "SubForgeで字幕を作成",
        _ => "Create subtitles with SubForge",
    };
    for extension in [
        ".mkv", ".mp4", ".avi", ".mov", ".webm", ".m4v", ".wmv", ".mpg", ".mpeg", ".ts", ".mts",
        ".m2ts", ".flv", ".ogv", ".vob", ".3gp",
    ] {
        let path =
            format!("Software\\Classes\\SystemFileAssociations\\{extension}\\shell\\SubForge");
        if enabled {
            let (key, _) = root.create_subkey(&path)?;
            key.set_value("", &title)?;
            key.set_value("Icon", &format!("\"{}\",0", exe.display()))?;
            key.set_value("MultiSelectModel", &"Player")?;
            key.create_subkey("command")?.0.set_value("", &cmd)?;
        } else {
            let _ = root.delete_subkey_all(&path);
        }
    }
    let path = "Software\\Classes\\Directory\\shell\\SubForge";
    if enabled {
        let (key, _) = root.create_subkey(path)?;
        key.set_value("", &title)?;
        key.set_value("Icon", &format!("\"{}\",0", exe.display()))?;
        key.set_value("MultiSelectModel", &"Player")?;
        key.create_subkey("command")?.0.set_value("", &cmd)?;
    } else {
        let _ = root.delete_subkey_all(path);
    }
    Ok(())
}
fn user_data_dir() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("SUBFORGE_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(PathBuf::from(
        std::env::var_os("APPDATA").ok_or_else(|| anyhow::anyhow!("profile_missing"))?,
    )
    .join("gr.sidos.subforge"))
}
fn delete_user_data() -> anyhow::Result<()> {
    let dir = user_data_dir()?;
    anyhow::ensure!(
        dir.is_absolute() && dir.parent().is_some(),
        "invalid_data_directory"
    );
    if let Ok(bytes) = std::fs::read(dir.join("settings.json")) {
        if let Ok(settings) = serde_json::from_slice::<Settings>(&bytes) {
            for p in settings.speech.iter().chain(&settings.translation) {
                let _ = providers::set_key(&p.id, "");
            }
        }
    }
    if dir.exists() {
        let canonical = std::fs::canonicalize(&dir)?;
        let parent = std::fs::canonicalize(dir.parent().unwrap())?;
        anyhow::ensure!(
            canonical.starts_with(&parent) && canonical != parent,
            "invalid_data_directory"
        );
        std::fs::remove_dir_all(canonical)?;
    }
    Ok(())
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--register-shell") {
        let settings = user_data_dir()
            .ok()
            .and_then(|dir| std::fs::read(dir.join("settings.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
            .unwrap_or_default();
        let _ = shell(settings.shell_enabled, &settings.ui_language);
        return;
    }
    if args.iter().any(|a| a == "--unregister-shell") {
        let _ = shell(false, "en");
        return;
    }
    if args.iter().any(|a| a == "--delete-user-data") {
        let _ = delete_user_data();
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(state) = app.try_state::<AppState>() {
                let paths = args
                    .into_iter()
                    .skip(1)
                    .filter(|a| !a.starts_with("--"))
                    .collect();
                let _ = state.add(paths);
                state.running.store(true, Ordering::SeqCst);
                state.emit();
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                if let Some(state) = window.try_state::<AppState>() {
                    state.logins.lock().unwrap().clear();
                    openai_oauth::cancel_all(&state);
                }
            }
        })
        .setup(move |app| {
            let state = Engine::new(app.handle().clone())?;
            state.scheduler();
            let paths = args
                .iter()
                .skip(1)
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                state.add(paths)?;
                state.running.store(true, Ordering::SeqCst);
            }
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            snapshot,
            save_settings,
            save_key,
            key_saved,
            add_files,
            start_queue,
            job_action,
            clear_finished,
            inspect_file,
            download_model,
            remove_model,
            provider_models,
            test_provider,
            export_settings,
            import_settings,
            open_output,
            diagnostics,
            job_log,
            export_job_log,
            open_provider_setup,
            prepare_agent,
            agent_signin,
            agent_signin_action,
            openai_signin,
            openai_status,
            openai_cancel,
            openai_signout,
            openai_acknowledge,
            check_updates,
            update_status,
            install_update
        ])
        .run(tauri::generate_context!())
        .expect("SubForge could not start");
}
