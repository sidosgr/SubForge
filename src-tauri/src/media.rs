use crate::{
    engine::{Control, Engine},
    subtitles,
    types::{Cue, Settings, Transcript},
};
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::io::AsyncReadExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stream {
    pub index: usize,
    pub kind: String,
    pub codec: String,
    pub language: String,
    pub title: String,
    pub default: bool,
    pub forced: bool,
    pub commentary: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Probe {
    pub duration: f64,
    pub streams: Vec<Stream>,
}
pub async fn command(
    cmd: tokio::process::Command,
    control: &Control,
    timeout: u64,
) -> Result<String> {
    Ok(command_output(cmd, control, timeout).await?.0)
}
async fn command_output(
    mut cmd: tokio::process::Command,
    control: &Control,
    timeout: u64,
) -> Result<(String, String)> {
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|_| anyhow::anyhow!("dependency_missing"))?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = tokio::spawn(async move {
        let mut v = Vec::new();
        stdout.read_to_end(&mut v).await.map(|_| v)
    });
    let err = tokio::spawn(async move {
        let mut v = Vec::new();
        stderr.read_to_end(&mut v).await.map(|_| v)
    });
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(timeout);
    loop {
        if control.canceled() {
            stop_process(&mut child).await;
            out.abort();
            err.abort();
            bail!("canceled");
        }
        if let Some(status) = child.try_wait()? {
            let output = out.await??;
            let error = err.await??;
            // OpenCode run can report an error event and still exit successfully.
            // Propagate that failure so retries and provider fallback remain active.
            let structured_error = String::from_utf8_lossy(&output).lines().any(|line| {
                serde_json::from_str::<Value>(line)
                    .is_ok_and(|v| v["type"] == "error" || v["status"] == "ERROR")
            });
            if !status.success() || structured_error {
                bail!("{}", process_error(&output, &error));
            }
            return Ok((
                String::from_utf8_lossy(&output).into_owned(),
                String::from_utf8_lossy(&error).into_owned(),
            ));
        }
        if tokio::time::Instant::now() > deadline {
            stop_process(&mut child).await;
            out.abort();
            err.abort();
            bail!("request_timeout");
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}
fn process_error(output: &[u8], error: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(output);
    let stderr = String::from_utf8_lossy(error);
    // Structured CLI errors can be on stdout. Retain only status and message;
    // response bodies, request metadata and headers never enter app diagnostics.
    for line in stdout.lines().rev() {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if value["status"] == "ERROR" {
                let message = value["error"].as_str().unwrap_or("cli_failed");
                if message.contains("API key not valid")
                    || message.contains("authentication required")
                {
                    return "authentication_failed".into();
                }
                if let Some(code) = regex::Regex::new(r"Error (\d{3})")
                    .ok()
                    .and_then(|r| r.captures(message).map(|c| c[1].to_string()))
                {
                    return format!("http_{code}:60");
                }
                return format!(
                    "cli_failed: {}",
                    message
                        .split(", Details:")
                        .next()
                        .unwrap_or("cli_failed")
                        .chars()
                        .take(300)
                        .collect::<String>()
                );
            }
            if value["type"] == "error" {
                let data = &value["error"]["data"];
                let message = data["message"]
                    .as_str()
                    .or_else(|| value["error"]["message"].as_str())
                    .unwrap_or("cli_failed");
                let message: String = message.chars().take(300).collect();
                if let Some(code) = data["statusCode"].as_u64() {
                    return format!("http_{code}:60: {message}");
                }
                return format!("cli_failed: {message}");
            }
        }
    }
    let diagnostic = if stderr.trim().is_empty() {
        stdout.as_ref()
    } else {
        stderr.as_ref()
    };
    if let Ok(pattern) = regex::Regex::new(r"(?:^|\s)(401|403|429|500|502|503|504):") {
        if let Some(code) = pattern.captures(diagnostic) {
            return format!("http_{}:60", &code[1]);
        }
    }
    format!(
        "process_failed: {}",
        diagnostic
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(600)
            .collect::<String>()
    )
}
async fn stop_process(child: &mut tokio::process::Child) {
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let mut stop = tokio::process::Command::new("taskkill.exe");
        stop.creation_flags(0x08000000)
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), stop.status()).await;
    }
    let _ = child.kill().await;
}
pub async fn probe(engine: &Engine, path: &Path, control: &Control) -> Result<Probe> {
    let mut cmd = tokio::process::Command::new(engine.binary("ffprobe.exe"));
    cmd.args([
        "-v",
        "error",
        "-show_format",
        "-show_streams",
        "-of",
        "json",
    ])
    .arg(path);
    let text = command(cmd, control, 60).await?;
    let v: Value = serde_json::from_str(&text)?;
    let duration = v["format"]["duration"]
        .as_str()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0.);
    ensure!(duration > 0., "invalid_media");
    let streams = v["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid_media"))?
        .iter()
        .map(|s| Stream {
            index: s["index"].as_u64().unwrap_or(0) as usize,
            kind: s["codec_type"].as_str().unwrap_or("").into(),
            codec: s["codec_name"].as_str().unwrap_or("").into(),
            language: normalize_language(s["tags"]["language"].as_str().unwrap_or("auto")),
            title: s["tags"]["title"].as_str().unwrap_or("").into(),
            default: s["disposition"]["default"].as_u64() == Some(1),
            forced: s["disposition"]["forced"].as_u64() == Some(1),
            commentary: s["disposition"]["comment"].as_u64() == Some(1)
                || s["tags"]["title"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("commentary"),
        })
        .collect();
    Ok(Probe { duration, streams })
}
pub fn normalize_language(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "eng" | "english" | "en-us" | "en-gb" => "en",
        "gre" | "ell" | "greek" | "ελληνικά" => "el",
        "spa" | "spanish" | "español" => "es",
        "fra" | "fre" | "french" | "français" => "fr",
        "deu" | "ger" | "german" | "deutsch" => "de",
        "por" | "portuguese" | "português" | "pt-br" | "pt-pt" => "pt",
        "ita" | "italian" | "italiano" => "it",
        "zho" | "chi" | "zh" | "zh-cn" | "zh-hans" | "chinese" | "mandarin" => "zh-Hans",
        "kor" | "korean" => "ko",
        "jpn" | "japanese" => "ja",
        "und" | "" => "auto",
        _ => s,
    }
    .into()
}
pub fn audio_track(probe: &Probe, settings: &Settings) -> Result<usize> {
    let audio: Vec<_> = probe.streams.iter().filter(|s| s.kind == "audio").collect();
    if let Some(index) = settings.audio_track {
        ensure!(
            audio.iter().any(|s| s.index == index),
            "audio_track_missing"
        );
        return Ok(index);
    }
    audio
        .into_iter()
        .max_by_key(|s| {
            (
                !s.commentary,
                settings.source_language != "auto" && s.language == settings.source_language,
                s.default,
            )
        })
        .map(|s| s.index)
        .ok_or_else(|| anyhow::anyhow!("no_audio"))
}
pub async fn extract(
    engine: &Engine,
    path: &Path,
    index: usize,
    start: f64,
    duration: f64,
    destination: &Path,
    control: &Control,
) -> Result<()> {
    let temporary = destination.with_extension("partial.wav");
    let mut cmd = tokio::process::Command::new(engine.binary("ffmpeg.exe"));
    cmd.args(["-nostdin", "-v", "error", "-y", "-ss"])
        .arg(start.to_string())
        .arg("-i")
        .arg(path)
        .args([
            "-map",
            &format!("0:{index}"),
            "-t",
            &duration.to_string(),
            "-vn",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&temporary);
    let result = command(cmd, control, 300).await;
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    std::fs::rename(&temporary, destination)?;
    Ok(())
}
#[derive(Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub language: String,
    pub score: i32,
}
pub fn discover(path: &Path, settings: &Settings, duration: f64) -> Result<Vec<Candidate>> {
    if !settings.subtitle_override.is_empty() {
        let p = PathBuf::from(&settings.subtitle_override);
        let c = subtitles::read(&p)?;
        subtitles::validate(&c, duration)?;
        return Ok(vec![Candidate {
            language: settings.source_language.clone(),
            path: p,
            score: 1000,
        }]);
    }
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut dirs = vec![
        parent.to_path_buf(),
        parent.join("Subs"),
        parent.join("Subtitles"),
        parent.join(&stem),
    ];
    if !settings.subtitle_folder.is_empty() {
        dirs.push(PathBuf::from(&settings.subtitle_folder));
    }
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_file() || !seen.insert(p.clone()) {
                    continue;
                }
                let ext = p
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if !matches!(ext.as_str(), "srt" | "vtt" | "ass" | "ssa") {
                    continue;
                }
                let name = p
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                let matches = name == stem
                    || name
                        .strip_prefix(&stem)
                        .is_some_and(|suffix| suffix.starts_with(['.', ' ', '_', '-']));
                if !matches {
                    continue;
                }
                if name.contains("forced") || name.contains("signs") {
                    continue;
                }
                let Ok(cues) = subtitles::read(&p) else {
                    continue;
                };
                if subtitles::validate(&cues, duration).is_err() {
                    continue;
                }
                let last = cues.last().unwrap().end;
                let speech_span = cues.iter().map(|c| c.end - c.start).sum::<f64>();
                if duration > 120.
                    && (cues.len() < 5 || last < duration * 0.3 || speech_span < duration * 0.01)
                {
                    continue;
                }
                let suffix = name.strip_prefix(&stem).unwrap_or("");
                let mut language = "auto".to_string();
                for token in suffix.split(['.', '_', ' ', '-']) {
                    let normalized = normalize_language(token);
                    if crate::types::LANGUAGES
                        .iter()
                        .any(|(c, _)| *c == normalized)
                    {
                        language = normalized;
                    }
                }
                if suffix.contains("zh-hans") {
                    language = "zh-Hans".into();
                }
                let score = if language == settings.target_language {
                    100
                } else if language == settings.source_language && language != "auto" {
                    70
                } else {
                    40
                };
                result.push(Candidate {
                    path: p,
                    language,
                    score,
                });
            }
        }
    }
    result.sort_by_key(|c| -c.score);
    Ok(result)
}
pub async fn embedded(
    engine: &Engine,
    path: &Path,
    probe: &Probe,
    settings: &Settings,
    dir: &Path,
    control: &Control,
) -> Result<Option<(Transcript, String)>> {
    let mut streams: Vec<_> = probe
        .streams
        .iter()
        .filter(|s| {
            s.kind == "subtitle"
                && !s.forced
                && matches!(
                    s.codec.as_str(),
                    "subrip" | "ass" | "ssa" | "webvtt" | "mov_text" | "text"
                )
        })
        .collect();
    streams.sort_by_key(|s| {
        -(if s.language == settings.target_language {
            100
        } else if s.language == settings.source_language && s.language != "auto" {
            70
        } else if s.default {
            50
        } else {
            40
        })
    });
    for s in streams {
        let dest = dir.join(format!("embedded-{}.srt", s.index));
        let mut cmd = tokio::process::Command::new(engine.binary("ffmpeg.exe"));
        cmd.args(["-nostdin", "-v", "error", "-y", "-i"])
            .arg(path)
            .args(["-map", &format!("0:{}", s.index), "-c:s", "srt"])
            .arg(&dest);
        if command(cmd, control, 120).await.is_err() {
            continue;
        }
        if let Ok(cues) = subtitles::read(&dest) {
            if subtitles::validate(&cues, probe.duration).is_ok()
                && !(probe.duration > 120.
                    && (cues.len() < 5 || cues.last().unwrap().end < probe.duration * 0.3))
            {
                return Ok(Some((
                    Transcript {
                        language: s.language.clone(),
                        cues,
                    },
                    format!("embedded:{}", s.index),
                )));
            }
        }
    }
    Ok(None)
}
pub async fn local(
    engine: &Engine,
    audio: &Path,
    model: &str,
    source: &str,
    dir: &Path,
    control: &Control,
    job_id: &str,
    duration: f64,
) -> Result<Transcript> {
    engine.note(job_id, "waiting_local", serde_json::json!({}));
    let _guard = tokio::select! { lock=engine.local_lock.lock()=>lock, _=control.until_canceled()=>bail!("canceled") };
    control.checkpoint().await?;
    engine.note(job_id, "checking_model", serde_json::json!({"model":model}));
    let installed = engine.dir.join("models").join(format!("ggml-{model}.bin"));
    ensure!(installed.exists(), "model_missing");
    let expected = crate::engine::MODEL_DATA
        .iter()
        .find(|(id, _, _)| *id == model)
        .ok_or_else(|| anyhow::anyhow!("invalid_model"))?;
    ensure!(
        crate::engine::file_hash(&installed)? == expected.2,
        "model_checksum_failed"
    );
    engine.note(
        job_id,
        "recognizing",
        serde_json::json!({"model":model,"seconds":duration}),
    );
    let link = dir.join("model.bin");
    if !link.exists() {
        std::fs::hard_link(&installed, &link)
            .or_else(|_| std::fs::copy(&installed, &link).map(|_| ()))?;
    }
    let output = dir.join("local.srt");
    if output.exists() {
        std::fs::remove_file(&output)?;
    }
    let mut cmd = tokio::process::Command::new(engine.binary("ffmpeg.exe"));
    let lang = if source == "zh-Hans" { "zh" } else { source };
    cmd.current_dir(dir).args(["-nostdin","-nostats","-v","debug","-filter_threads","4","-i"]).arg(audio).args(["-af",&format!("whisper=model=model.bin:language={lang}:translate=false:queue=25:use_gpu=false:destination=local.srt:format=srt:max_len=84"),"-f","null","-"]);
    let result = command_output(cmd, control, 3600).await;
    let _ = std::fs::remove_file(&link);
    let (_, diagnostic) = result?;
    ensure!(
        !diagnostic.contains("Failed to process audio with whisper.cpp"),
        "local_transcription_failed"
    );
    let language = if source == "auto" {
        regex::Regex::new(r"auto-detected language:\s*([a-z]{2,3})")?
            .captures(&diagnostic)
            .map(|c| normalize_language(&c[1]))
            .unwrap_or_else(|| source.into())
    } else {
        source.into()
    };
    let cues = if !output.exists() || std::fs::metadata(&output)?.len() == 0 {
        Vec::new()
    } else {
        let (cues, repaired) = subtitles::read_whisper(&output, duration)?;
        if repaired.zero_duration > 0 {
            engine.note(
                job_id,
                "timing_repaired",
                serde_json::json!({"count":repaired.zero_duration}),
            );
        }
        if !repaired.boundary_clips.is_empty() {
            engine.note(job_id,"timing_clipped",serde_json::json!({"count":repaired.boundary_clips.len(),"duration":duration,"repairs":repaired.boundary_clips}));
        }
        cues
    };
    Ok(Transcript { language, cues })
}
pub fn speech_json(v: &Value, source: &str) -> Result<Transcript> {
    let language = normalize_language(v["language"].as_str().unwrap_or(source));
    if let Some(vtt) = v["vtt"].as_str() {
        return Ok(Transcript {
            language,
            cues: subtitles::parse(vtt, "vtt")?,
        });
    }
    let segments = v["segments"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing_timestamps"))?;
    let mut cues = Vec::new();
    for s in segments {
        let start = s["start"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("missing_timestamps"))?;
        let end = s["end"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("missing_timestamps"))?;
        let text = s["text"].as_str().unwrap_or("").trim();
        if !text.is_empty() && end > start {
            cues.push(Cue {
                id: cues.len() as u32 + 1,
                start,
                end,
                text: text.into(),
            });
        }
    }
    if !cues.is_empty() {
        subtitles::validate(&cues, 0.)?;
    }
    Ok(Transcript { language, cues })
}
// The extractor always writes 16-bit mono PCM. Only exact/near digital silence is
// skipped here; low-volume speech is still sent to the chosen recognizer.
pub fn silent_wav(path: &Path) -> Result<bool> {
    let bytes = std::fs::read(path)?;
    ensure!(
        bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"),
        "invalid_audio"
    );
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into()?) as usize;
        let start = offset + 8;
        let end = start.saturating_add(size).min(bytes.len());
        if &bytes[offset..offset + 4] == b"data" {
            return Ok(bytes[start..end]
                .chunks_exact(2)
                .all(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs() <= 2));
        }
        offset = start.saturating_add(size).saturating_add(size % 2);
    }
    bail!("invalid_audio")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefers_main_audio() {
        let p = Probe {
            duration: 100.,
            streams: vec![
                Stream {
                    index: 1,
                    kind: "audio".into(),
                    codec: "aac".into(),
                    language: "en".into(),
                    title: "commentary".into(),
                    default: true,
                    forced: false,
                    commentary: true,
                },
                Stream {
                    index: 2,
                    kind: "audio".into(),
                    codec: "aac".into(),
                    language: "en".into(),
                    title: "main".into(),
                    default: false,
                    forced: false,
                    commentary: false,
                },
            ],
        };
        assert_eq!(audio_track(&p, &Settings::default()).unwrap(), 2);
    }
    #[test]
    fn empty_cloud_segments_are_valid_silent_chunks() {
        let transcript =
            speech_json(&serde_json::json!({"language":"en","segments":[]}), "auto").unwrap();
        assert!(transcript.cues.is_empty());
        assert!(speech_json(&serde_json::json!({"text":"No timestamps"}), "auto").is_err());
    }
    #[test]
    fn discovery_matches_unicode_stems_and_rejects_forced_and_unrelated() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(dir.join("Subs")).unwrap();
        let text = "1\n00:00:01,000 --> 00:00:03,000\nDialogue\n";
        for name in ["Δείγμα.en.srt", "Δείγμα.el.forced.srt", "Unrelated.el.srt"] {
            std::fs::write(dir.join(name), text).unwrap();
        }
        std::fs::write(dir.join("Subs/Δείγμα.el.srt"), text).unwrap();
        let candidates = discover(&dir.join("Δείγμα.mkv"), &Settings::default(), 20.).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].language, "el");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cli_error_reports_status_without_headers_or_response_body() {
        let payload=serde_json::json!({"type":"error","error":{"data":{"statusCode":403,"message":"Free tier requires sign-in","responseHeaders":{"secret":"hidden-key"},"responseBody":"private body"}}}).to_string();
        let error = process_error(payload.as_bytes(), b"");
        assert!(error.starts_with("http_403:60:"));
        assert!(!error.contains("hidden-key"));
        assert!(!error.contains("private body"));
    }
    #[test]
    fn verbose_speech_language_names_match_target_codes() {
        let value = serde_json::json!({"language":"english","segments":[{"start":0.1,"end":1.0,"text":"Hello."}]});
        assert_eq!(speech_json(&value, "auto").unwrap().language, "en");
        assert_eq!(normalize_language("Greek"), "el");
    }
    #[test]
    fn antigravity_authentication_error_excludes_request_metadata() {
        let payload=serde_json::json!({"status":"ERROR","error":"Error 400, Message: API key not valid., Details: private headers"}).to_string();
        assert_eq!(
            process_error(payload.as_bytes(), b"private stderr"),
            "authentication_failed"
        );
    }
}
