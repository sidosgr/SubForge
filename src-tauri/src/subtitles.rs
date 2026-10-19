use crate::types::{Cue, Transcript};
use anyhow::{bail, ensure, Result};
use regex::Regex;
use std::{fs, path::Path};

pub fn read(path: &Path) -> Result<Vec<Cue>> {
    parse(
        &read_text(path)?,
        path.extension().and_then(|s| s.to_str()).unwrap_or("srt"),
    )
}
fn read_text(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let text = if bytes.starts_with(&[0xff, 0xfe]) {
        encoding_rs::UTF_16LE.decode(&bytes[2..]).0.into_owned()
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        encoding_rs::UTF_16BE.decode(&bytes[2..]).0.into_owned()
    } else {
        match String::from_utf8(bytes.clone()) {
            Ok(s) => s,
            Err(_) => bail!("subtitle_encoding: {}", path.display()),
        }
    };
    Ok(text)
}
pub fn time(s: &str) -> Result<f64> {
    let v = s.trim().replace(',', ".");
    let p: Vec<_> = v.split(':').collect();
    match p.len() {
        3 => Ok(p[0].parse::<f64>()? * 3600.0
            + p[1].parse::<f64>()? * 60.0
            + p[2].parse::<f64>()?),
        2 => Ok(p[0].parse::<f64>()? * 60.0 + p[1].parse::<f64>()?),
        _ => bail!("invalid_timestamp"),
    }
}
pub fn parse(text: &str, format: &str) -> Result<Vec<Cue>> {
    let cues = parse_cues(text, format)?;
    validate(&cues, 0.0)?;
    Ok(cues)
}
fn parse_cues(text: &str, format: &str) -> Result<Vec<Cue>> {
    let clean = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut cues = Vec::new();
    if matches!(format, "ass" | "ssa") {
        let tags = Regex::new(r"\{[^}]*\}")?;
        let mut columns = vec![
            "layer", "start", "end", "style", "name", "marginl", "marginr", "marginv", "effect",
            "text",
        ];
        let mut events = false;
        for line in clean.lines() {
            if line.starts_with('[') {
                events = line.trim().eq_ignore_ascii_case("[Events]");
                continue;
            }
            if !events {
                continue;
            }
            if let Some(f) = line.strip_prefix("Format:") {
                columns = f.split(',').map(|x| x.trim()).collect();
                continue;
            }
            if let Some(d) = line.strip_prefix("Dialogue:") {
                let parts: Vec<_> = d.trim().splitn(columns.len(), ',').collect();
                let column = |name: &str| columns.iter().position(|c| c.eq_ignore_ascii_case(name));
                if let (Some(a), Some(b), Some(t)) =
                    (column("start"), column("end"), column("text"))
                {
                    if parts.len() > t.max(a).max(b) {
                        cues.push(Cue {
                            id: cues.len() as u32 + 1,
                            start: time(parts[a])?,
                            end: time(parts[b])?,
                            text: tags
                                .replace_all(parts[t], "")
                                .replace("\\N", "\n")
                                .replace("\\n", "\n")
                                .replace("\\h", " "),
                        });
                    }
                }
            }
        }
    } else {
        let timing = Regex::new(
            r"(?m)^(\d{1,2}:\d{2}(?::\d{2})?[.,]\d{1,3})\s*-->\s*(\d{1,2}:\d{2}(?::\d{2})?[.,]\d{1,3})[^\n]*\n",
        )?;
        let matches: Vec<_> = timing.captures_iter(&clean).collect();
        for (i, c) in matches.iter().enumerate() {
            let begin = c.get(0).unwrap().end();
            let end = matches
                .get(i + 1)
                .map(|n| n.get(0).unwrap().start())
                .unwrap_or(clean.len());
            let block = &clean[begin..end];
            let text = block.split("\n\n").next().unwrap_or("").trim();
            if !text.is_empty() {
                cues.push(Cue {
                    id: cues.len() as u32 + 1,
                    start: time(&c[1])?,
                    end: time(&c[2])?,
                    text: text.into(),
                });
            }
        }
    }
    ensure!(!cues.is_empty(), "empty_subtitles");
    Ok(cues)
}
/// Only recognition output may contain zero-duration Whisper artifacts. External
/// subtitles and provider checkpoints still pass the strict validator.
#[derive(Default)]
pub struct RecognitionRepairs {
    pub zero_duration: usize,
    pub boundary_clips: Vec<serde_json::Value>,
}
pub fn read_whisper(path: &Path, duration: f64) -> Result<(Vec<Cue>, RecognitionRepairs)> {
    ensure!(
        duration.is_finite() && duration > 0.0,
        "invalid_audio_duration"
    );
    let mut cues = parse_cues(&read_text(path)?, "srt")?;
    let mut repaired = RecognitionRepairs::default();
    for i in 0..cues.len() {
        let c = &mut cues[i];
        // The final Whisper segment may extend past a short final inference
        // window. Intersect a cue that starts inside real audio with its boundary;
        // retain its ID, start and text. Never move an entirely outside cue inward.
        if c.start.is_finite()
            && c.end.is_finite()
            && c.start >= 0.0
            && c.start < duration
            && c.end > duration
        {
            repaired.boundary_clips.push(serde_json::json!({"cue":c.id,"start":c.start,"original_end":c.end,"end":duration,"duration":duration}));
            c.end = duration;
        }
        let c = &cues[i];
        if c.start.is_finite() && c.start >= 0.0 && c.end == c.start {
            let limit = cues
                .get(i + 1)
                .filter(|n| n.start > c.start)
                .map(|n| n.start)
                .unwrap_or(f64::INFINITY)
                .min(duration);
            let end = (c.start + 0.2).min(limit);
            ensure!(
                end > c.start,
                "invalid_timestamp: recognition cue {} has no available interval",
                c.id
            );
            cues[i].end = end;
            repaired.zero_duration += 1;
        }
    }
    for c in &cues {
        ensure!(
            c.start < duration && c.end <= duration,
            "subtitle_out_of_bounds: cue {} start={} end={} duration={}",
            c.id,
            c.start,
            c.end,
            duration
        );
    }
    validate(&cues, duration)?;
    Ok((cues, repaired))
}
pub fn validate(cues: &[Cue], duration: f64) -> Result<()> {
    ensure!(!cues.is_empty(), "no_speech");
    let mut prev = -1.0;
    let mut ids = std::collections::HashSet::new();
    for c in cues {
        ensure!(
            c.start.is_finite() && c.end.is_finite() && c.start >= 0.0 && c.end > c.start,
            "invalid_timestamp: cue {} start={} end={}",
            c.id,
            c.start,
            c.end
        );
        ensure!(
            c.start >= prev,
            "subtitle_order: cue {} start={} previous_start={}",
            c.id,
            c.start,
            prev
        );
        ensure!(
            ids.insert(c.id) && !c.text.trim().is_empty(),
            "invalid_segments"
        );
        if duration > 0.0 {
            ensure!(
                c.end <= duration + 2.0,
                "subtitle_out_of_bounds: cue {} start={} end={} duration={}",
                c.id,
                c.start,
                c.end,
                duration
            );
        }
        prev = c.start;
    }
    Ok(())
}
fn stamp(t: f64, separator: char) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}{}{:03}",
        ms / 3600000,
        (ms / 60000) % 60,
        (ms / 1000) % 60,
        separator,
        ms % 1000
    )
}
pub fn render(cues: &[Cue], format: &str) -> String {
    let mut out = if format == "vtt" {
        "WEBVTT\n\n".into()
    } else {
        String::new()
    };
    for (i, c) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            stamp(c.start, if format == "vtt" { '.' } else { ',' }),
            stamp(c.end, if format == "vtt" { '.' } else { ',' }),
            wrap(&c.text)
        ));
    }
    out
}
fn wrap(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let mut current = String::new();
        if line.contains(' ') {
            for word in line.split_whitespace() {
                if !current.is_empty() && current.chars().count() + word.chars().count() + 1 > 44 {
                    lines.push(std::mem::take(&mut current));
                }
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        } else {
            for ch in line.chars() {
                if current.chars().count() >= 24 {
                    lines.push(std::mem::take(&mut current));
                }
                current.push(ch);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines.join("\n")
}
pub fn merge(mut chunks: Vec<(f64, Transcript)>) -> Transcript {
    let language = chunks
        .iter()
        .map(|(_, t)| t.language.as_str())
        .find(|x| *x != "auto" && !x.is_empty())
        .unwrap_or("auto")
        .to_string();
    let mut cues: Vec<Cue> = Vec::new();
    chunks.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (offset, t) in chunks {
        for mut c in t.cues {
            c.start += offset;
            c.end += offset;
            if cues
                .iter()
                .rev()
                .take(6)
                .any(|p| (p.start - c.start).abs() < 3.0 && p.text.trim() == c.text.trim())
            {
                continue;
            }
            if let Some(prev) = cues.last_mut() {
                if c.start < prev.end && c.start >= prev.start {
                    prev.end = c.start.max(prev.start + 0.01);
                }
            }
            cues.push(c);
        }
    }
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    for (i, c) in cues.iter_mut().enumerate() {
        c.id = i as u32 + 1;
    }
    Transcript { language, cues }
}
pub fn translation_result(value: &serde_json::Value, source: &[Cue]) -> Result<Vec<Cue>> {
    let segments = value
        .get("segments")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("invalid_segments"))?;
    ensure!(segments.len() == source.len(), "missing_segments");
    let mut map = std::collections::HashMap::new();
    for v in segments {
        let id: u32 = v["id"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("invalid_segments"))?
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid_segments"))?;
        let text = v["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid_segments"))?;
        ensure!(
            !text.trim().is_empty() && map.insert(id, text).is_none(),
            "invalid_segments"
        );
    }
    source
        .iter()
        .map(|c| {
            let mut result = c.clone();
            result.text = map
                .get(&c.id)
                .ok_or_else(|| anyhow::anyhow!("missing_segments"))?
                .to_string();
            Ok(result)
        })
        .collect()
}
pub fn json_text(text: &str) -> Result<serde_json::Value> {
    let text = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if let Ok(v) = serde_json::from_str(text) {
        return Ok(v);
    }
    json_candidates(text)
        .into_iter()
        .last()
        .ok_or_else(|| anyhow::anyhow!("invalid_json"))
}
// Agent replies can contain a preamble, repeated answers, or fenced JSON. Parse
// complete values separately rather than joining the first and last braces.
pub fn json_candidates(text: &str) -> Vec<serde_json::Value> {
    let mut values = Vec::new();
    let mut offset = 0;
    while let Some(relative) = text[offset..].find('{') {
        let start = offset + relative;
        let mut stream =
            serde_json::Deserializer::from_str(&text[start..]).into_iter::<serde_json::Value>();
        if let Some(Ok(value)) = stream.next() {
            offset = start + stream.byte_offset();
            values.push(value);
        } else {
            offset = start + 1;
        }
    }
    values
}
pub fn translation_batches(cues: &[Cue], maximum: usize) -> Vec<std::ops::Range<usize>> {
    let mut batches = Vec::new();
    let mut start = 0;
    while start < cues.len() {
        let mut end = start;
        let mut bytes = 0;
        while end < cues.len() && end - start < maximum.clamp(1, 8) {
            let next = cues[end].text.len();
            if end > start && bytes + next > 3000 {
                break;
            }
            bytes += next;
            end += 1;
        }
        batches.push(start..end);
        start = end;
    }
    batches
}
#[cfg(test)]
mod tests {
    #[test]
    fn reported_boundary_corpus_preserves_all_75_cues() {
        let text = include_str!("../tests/fixtures/whisper-boundary-overrun.srt");
        let before = parse_cues(text, "srt").unwrap();
        assert_eq!(before.len(), 75);
        assert!(validate(&before, 300.).is_err());
        let path = std::env::temp_dir().join(format!("{}.srt", uuid::Uuid::new_v4()));
        fs::write(&path, text).unwrap();
        let (after, repairs) = read_whisper(&path, 300.).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(after.len(), 75);
        assert_eq!(repairs.boundary_clips.len(), 1);
        for (a, b) in after.iter().zip(&before) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.start, b.start);
            assert_eq!(a.text, b.text);
            assert_eq!(a.end, b.end.min(300.));
        }
        assert_eq!(after[74].start, 299.52);
        assert_eq!(after[74].end, 300.);
        validate(&after, 300.).unwrap();
    }
    #[test]
    fn whisper_zero_duration_is_repaired_but_external_subtitles_stay_strict() {
        let text="1\n00:02:03,840 --> 00:02:03,840\nSynthetic word\n\n2\n00:02:04,000 --> 00:02:05,000\nNext\n";
        assert!(parse(text, "srt").is_err());
        let path = std::env::temp_dir().join(format!("{}.srt", uuid::Uuid::new_v4()));
        fs::write(&path, text).unwrap();
        let (cues, count) = read_whisper(&path, 150.).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(count.zero_duration, 1);
        assert_eq!(cues[0].start, 123.84);
        assert_eq!(cues[0].end, 124.0);
        assert_eq!(cues[0].text, "Synthetic word");
        validate(&cues, 150.).unwrap();
    }
    #[test]
    fn whisper_reversed_or_out_of_bounds_timing_is_not_fabricated() {
        let path = std::env::temp_dir().join(format!("{}.srt", uuid::Uuid::new_v4()));
        for text in [
            "1\n00:00:02,000 --> 00:00:01,000\nInvalid\n",
            "1\n00:00:10,000 --> 00:00:10,000\nAt the end\n",
            "1\n00:00:13,000 --> 00:00:15,000\nEntirely outside\n",
            "1\n00:00:10,200 --> 00:00:11,200\nOutside within generic tolerance\n",
        ] {
            fs::write(&path, text).unwrap();
            assert!(read_whisper(&path, 10.).is_err());
        }
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn whisper_tail_is_clipped_to_real_audio_without_losing_text_or_moving_start() {
        let path = std::env::temp_dir().join(format!("{}.srt", uuid::Uuid::new_v4()));
        fs::write(&path,"1\n00:04:56,560 --> 00:04:59,560\nPrevious\n\n2\n00:04:59,520 --> 00:05:09,520\nSynthetic final dialogue\n").unwrap();
        let (cues, repairs) = read_whisper(&path, 300.).unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[1].id, 2);
        assert_eq!(cues[1].start, 299.52);
        assert_eq!(cues[1].end, 300.);
        assert_eq!(cues[1].text, "Synthetic final dialogue");
        assert_eq!(repairs.zero_duration, 0);
        assert_eq!(repairs.boundary_clips.len(), 1);
        assert_eq!(repairs.boundary_clips[0]["original_end"], 309.52);
        validate(&cues, 300.).unwrap();
        let rendered = render(&cues, "srt");
        assert!(rendered.contains("00:04:59,520 --> 00:05:00,000"));
        let merged = merge(vec![(
            2086.,
            Transcript {
                language: "en".into(),
                cues,
            },
        )]);
        assert_eq!(merged.cues[1].end, 2386.);
        validate(&merged.cues, 2734.).unwrap();
    }
    use super::*;
    #[test]
    fn unicode_srt_and_vtt() {
        let c=parse("1\n00:00:01,250 --> 00:00:03,000\nΓεια σου 世界\n\n2\n00:00:04,000 --> 00:00:06,000\nSecond line\n","srt").unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(parse(&render(&c, "vtt"), "vtt").unwrap(), c);
    }
    #[test]
    fn translation_cannot_drop_or_duplicate() {
        let c = vec![
            Cue {
                id: 1,
                start: 1.,
                end: 2.,
                text: "one".into(),
            },
            Cue {
                id: 2,
                start: 2.,
                end: 3.,
                text: "two".into(),
            },
        ];
        assert!(translation_result(
            &serde_json::json!({"segments":[{"id":1,"text":"ένα"},{"id":1,"text":"δύο"}]}),
            &c
        )
        .is_err());
        let r = translation_result(
            &serde_json::json!({"segments":[{"id":2,"text":"δύο"},{"id":1,"text":"ένα"}]}),
            &c,
        )
        .unwrap();
        assert_eq!(r[0].start, 1.);
        assert_eq!(r[0].text, "ένα");
    }
    #[test]
    fn ass_format_and_commas() {
        let r=parse("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,{\\i1}Hello, world\\NAgain","ass").unwrap();
        assert_eq!(r[0].text, "Hello, world\nAgain");
    }
    #[test]
    fn boundary_duplicates_removed() {
        let c = Cue {
            id: 1,
            start: 9.,
            end: 11.,
            text: "hello".into(),
        };
        let r = merge(vec![
            (
                0.,
                Transcript {
                    language: "en".into(),
                    cues: vec![c.clone()],
                },
            ),
            (
                8.,
                Transcript {
                    language: "en".into(),
                    cues: vec![Cue {
                        start: 1.,
                        end: 3.,
                        ..c
                    }],
                },
            ),
        ]);
        assert_eq!(r.cues.len(), 1);
    }
}
