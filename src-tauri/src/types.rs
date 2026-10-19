use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub endpoint: String,
    pub models: Vec<String>,
    pub enabled: bool,
    pub cost: String,
    pub credit_enabled: bool,
    pub retries: u32,
    pub delay_ms: u64,
    pub timeout_secs: u64,
    pub account_id: String,
    pub cli_path: String,
    pub auth: String,
}
impl Default for Provider {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Custom".into(),
            kind: "custom".into(),
            endpoint: String::new(),
            models: vec![],
            enabled: false,
            cost: "unknown".into(),
            credit_enabled: false,
            retries: 2,
            delay_ms: 2000,
            timeout_secs: 180,
            account_id: String::new(),
            cli_path: String::new(),
            auth: "key".into(),
        }
    }
}
fn route(kind: &str, name: &str, endpoint: &str, model: &str, cost: &str) -> Provider {
    Provider {
        id: kind.into(),
        name: name.into(),
        kind: kind.into(),
        endpoint: endpoint.into(),
        models: vec![model.into()],
        cost: cost.into(),
        ..Default::default()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ui_language: String,
    pub target_language: String,
    pub source_language: String,
    pub theme: String,
    pub transcription_mode: String,
    pub local_model: String,
    pub cloud_fallback: bool,
    pub parallel_files: usize,
    pub allow_paid: bool,
    pub recursive: bool,
    pub output_directory: String,
    pub filename_pattern: String,
    pub output_format: String,
    pub overwrite: bool,
    pub retain_transcript: bool,
    pub audio_track: Option<usize>,
    pub subtitle_override: String,
    pub subtitle_folder: String,
    pub chunk_seconds: u64,
    pub batch_size: usize,
    pub speech: Vec<Provider>,
    pub translation: Vec<Provider>,
    pub shell_enabled: bool,
    pub retention_days: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_language: "en".into(),
            target_language: "el".into(),
            source_language: "auto".into(),
            theme: "dark".into(),
            transcription_mode: "online".into(),
            local_model: "base".into(),
            cloud_fallback: false,
            parallel_files: 1,
            allow_paid: false,
            recursive: false,
            output_directory: String::new(),
            filename_pattern: "{name}.{language}".into(),
            output_format: "srt".into(),
            overwrite: false,
            retain_transcript: false,
            audio_track: None,
            subtitle_override: String::new(),
            subtitle_folder: String::new(),
            chunk_seconds: 300,
            batch_size: 40,
            shell_enabled: true,
            retention_days: 7,
            speech: vec![
                route(
                    "groq",
                    "Groq",
                    "https://api.groq.com/openai/v1",
                    "whisper-large-v3-turbo",
                    "free",
                ),
                route(
                    "cloudflare",
                    "Cloudflare Workers AI",
                    "https://api.cloudflare.com/client/v4/accounts",
                    "@cf/openai/whisper-large-v3-turbo",
                    "free",
                ),
                route(
                    "gemini",
                    "Google Gemini",
                    "https://generativelanguage.googleapis.com/v1beta",
                    "gemini-2.5-flash",
                    "free",
                ),
                route(
                    "deepgram",
                    "Deepgram",
                    "https://api.deepgram.com/v1",
                    "nova-3",
                    "credit",
                ),
                route(
                    "speechmatics",
                    "Speechmatics",
                    "https://asr.api.speechmatics.com/v2",
                    "enhanced",
                    "credit",
                ),
            ],
            translation: vec![
                route("zen", "OpenCode Zen (CLI)", "", "", "unknown"),
                route(
                    "openrouter",
                    "OpenRouter",
                    "https://openrouter.ai/api/v1",
                    "",
                    "unknown",
                ),
                route("opencode", "OpenCode CLI", "", "", "unknown"),
                Provider {
                    auth: "oauth".into(),
                    ..route(
                        "antigravity",
                        "Antigravity CLI",
                        "",
                        "gemini-3.8-flash-low",
                        "free",
                    )
                },
                route("pi", "Pi agent", "", "", "unknown"),
            ],
        }
    }
}
impl Settings {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!((1..=8).contains(&self.parallel_files), "parallel_limit");
        anyhow::ensure!(
            matches!(self.local_model.as_str(), "base" | "small"),
            "invalid_model"
        );
        anyhow::ensure!((30..=300).contains(&self.chunk_seconds), "chunk_limit");
        anyhow::ensure!(
            matches!(self.theme.as_str(), "dark" | "light" | "system"),
            "invalid_theme"
        );
        anyhow::ensure!(
            self.source_language == "auto"
                || LANGUAGES.iter().any(|(c, _)| *c == self.source_language),
            "invalid_language"
        );
        anyhow::ensure!((1..=100).contains(&self.batch_size), "batch_limit");
        anyhow::ensure!(
            LANGUAGES
                .iter()
                .any(|(code, _)| *code == self.target_language),
            "invalid_language"
        );
        anyhow::ensure!(
            LANGUAGES.iter().any(|(code, _)| *code == self.ui_language),
            "invalid_language"
        );
        anyhow::ensure!(
            matches!(self.transcription_mode.as_str(), "local" | "online"),
            "invalid_mode"
        );
        anyhow::ensure!(
            matches!(self.output_format.as_str(), "srt" | "vtt"),
            "invalid_format"
        );
        anyhow::ensure!(
            !self
                .filename_pattern
                .contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']),
            "invalid_filename"
        );
        let mut ids = std::collections::HashSet::new();
        for p in self.speech.iter().chain(&self.translation) {
            anyhow::ensure!(
                !p.id.trim().is_empty()
                    && matches!(p.cost.as_str(), "free" | "credit" | "paid" | "unknown"),
                "invalid_provider"
            );
            anyhow::ensure!(ids.insert(&p.id), "duplicate_provider");
            anyhow::ensure!(
                p.retries <= 10
                    && p.timeout_secs >= 10
                    && p.timeout_secs <= 3600
                    && p.delay_ms <= 300000,
                "invalid_retry"
            );
            if !p.endpoint.is_empty() {
                let url = reqwest::Url::parse(&p.endpoint)?;
                anyhow::ensure!(
                    url.username().is_empty()
                        && url.password().is_none()
                        && url.query().is_none()
                        && url.fragment().is_none(),
                    "endpoint_credentials_forbidden"
                );
                anyhow::ensure!(
                    url.scheme() == "https"
                        || (url.scheme() == "http"
                            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
                    "https_required"
                );
            }
        }
        for p in &self.speech {
            anyhow::ensure!(
                matches!(
                    p.kind.as_str(),
                    "groq" | "cloudflare" | "gemini" | "deepgram" | "speechmatics" | "custom"
                ),
                "unsupported_speech_provider"
            );
        }
        for p in &self.translation {
            anyhow::ensure!(
                matches!(
                    p.kind.as_str(),
                    "zen"
                        | "openrouter"
                        | "opencode"
                        | "antigravity"
                        | "pi"
                        | "custom"
                        | "responses"
                        | "gemini_custom"
                ),
                "unsupported_translation_provider"
            );
        }
        Ok(())
    }
}
pub const LANGUAGES: [(&str, &str); 10] = [
    ("en", "English"),
    ("el", "Ελληνικά"),
    ("es", "Español"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("pt", "Português"),
    ("it", "Italiano"),
    ("zh-Hans", "简体中文"),
    ("ko", "한국어"),
    ("ja", "日本語"),
];
pub fn language_name(code: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .unwrap_or(code)
}
pub fn eligible(p: &Provider, s: &Settings) -> bool {
    p.enabled && (p.cost == "free" || (p.cost == "credit" && p.credit_enabled) || s.allow_paid)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cue {
    pub id: u32,
    pub start: f64,
    pub end: f64,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub language: String,
    pub cues: Vec<Cue>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub path: String,
    pub target: String,
    pub status: String,
    pub stage: String,
    pub progress: Option<f64>,
    pub provider: String,
    pub message: String,
    pub output: String,
    pub duration: f64,
    pub config: Settings,
    #[serde(default)]
    pub attempt: u32,
    #[serde(default)]
    pub started_at: f64,
    #[serde(default)]
    pub activity: serde_json::Value,
}
impl Job {
    pub fn new(path: String, config: Settings) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            target: config.target_language.clone(),
            path,
            status: "queued".into(),
            stage: "queued".into(),
            progress: None,
            provider: String::new(),
            message: String::new(),
            output: String::new(),
            duration: 0.0,
            config,
            attempt: 0,
            started_at: 0.0,
            activity: serde_json::Value::Null,
        }
    }
}
