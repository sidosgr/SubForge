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
                            text: plain_text(parts[t]),
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
            ids.insert(c.id) && has_visible_text(&c.text),
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
/// A single plain-text boundary for source captions, model replies and caches.
/// Font/position/size commands must never control the viewer's presentation.
pub fn plain_text(text: &str) -> String {
    static MARKUP: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static ENTITIES: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static INVISIBLE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static BLOCKS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static BREAKS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static DRAWINGS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let entities = ENTITIES.get_or_init(|| Regex::new(r"(?i)&(#x[0-9a-f]+|#[0-9]+|amp|lt|gt|quot|apos|nbsp|ensp|emsp|thinsp|ZeroWidthSpace|zwnj|zwj|lrm|rlm);").unwrap());
    let mut text = text.replace("\r\n", "\n").replace(['\r', '\u{2028}', '\u{2029}'], "\n");
    // Some providers copy already escaped markup. Decode before stripping it;
    // render() escapes any remaining literal angle brackets and ampersands.
    for _ in 0..8 {
        let decoded = entities.replace_all(&text, |c: &regex::Captures<'_>| {
            let value = c[1].to_ascii_lowercase();
            match value.as_str() {
                "amp" => "&".into(), "lt" => "<".into(), "gt" => ">".into(),
                "quot" => "\"".into(), "apos" => "'".into(),
                "nbsp" | "ensp" | "emsp" | "thinsp" => " ".into(),
                "zerowidthspace" | "zwnj" | "zwj" | "lrm" | "rlm" => String::new(),
                _ => {
                    let number = if let Some(hex) = value.strip_prefix("#x") {
                        u32::from_str_radix(hex, 16)
                    } else { value[1..].parse::<u32>() };
                    number.ok().and_then(char::from_u32).map(|ch| ch.to_string()).unwrap_or_default()
                }
            }
        }).into_owned();
        if decoded == text { break; }
        text = decoded;
    }
    let text = BLOCKS.get_or_init(|| Regex::new(r"(?is)<!--.*?(?:-->|$)|<(?:style|script)\b[^>]*>.*?(?:</(?:style|script)\s*>|$)").unwrap()).replace_all(&text, "");
    let text = DRAWINGS.get_or_init(|| Regex::new(r"(?is)\{[^}]*\\p[1-9][0-9]*\b[^}]*\}.*?(?:\{[^}]*\\p0\b[^}]*\}|$)").unwrap()).replace_all(&text, "");
    let text = BREAKS.get_or_init(|| Regex::new(r"(?i)<br\b[^>]*>|</?p\b[^>]*>").unwrap()).replace_all(&text, "\n");
    let text = MARKUP.get_or_init(|| Regex::new(r"(?s)</?[a-zA-Z][^>]*>|\{\\[^}]*\}").unwrap()).replace_all(&text, "");
    let text = INVISIBLE.get_or_init(|| Regex::new(r"[\p{Cf}\x{115f}\x{1160}\x{2800}\x{3164}\x{ffa0}]").unwrap()).replace_all(&text, "");
    let text = text.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ");
    let text: String = text.chars().map(|c| if c.is_control() && c != '\n' { ' ' } else { c }).collect();
    text.lines().map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n")
}
fn visible_text(text: &str) -> String {
    plain_text(text)
}
fn has_visible_text(text: &str) -> bool {
    static VISIBLE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    VISIBLE.get_or_init(|| Regex::new(r"[\p{L}\p{N}\p{P}\p{S}]").unwrap())
        .is_match(&visible_text(text))
}

/// Language labels are hints, never proof of the language of the dialogue.
/// Script checks catch transliteration even when statistical detection is uncertain.
pub fn validate_language(cues: &[Cue], target: &str) -> Result<()> {
    let expected = match target {
        "en" => "eng", "el" => "ell", "es" => "spa", "fr" => "fra",
        "de" => "deu", "pt" => "por", "it" => "ita", "zh-Hans" => "cmn",
        "ko" => "kor", "ja" => "jpn", _ => bail!("invalid_target_language"),
    };
    // Validate every character, not a percentage of each cue: even one
    // foreign-script word or emoji can otherwise leak into a valid sentence.
    static SCRIPTS: std::sync::OnceLock<Vec<Regex>> = std::sync::OnceLock::new();
    let scripts = SCRIPTS.get_or_init(|| [
        r"^[\p{Greek}\u0300-\u036f0-9\p{P}\p{Zs}\r\n\t\p{Sc}+=]*$",
        r"^[\p{Han}0-9\p{P}\p{Zs}\r\n\t\p{Sc}+=]*$",
        r"^[\p{Han}\p{Hiragana}\p{Katakana}\u3099\u309a\u30fc\uff700-9\p{P}\p{Zs}\r\n\t\p{Sc}+=]*$",
        r"^[\p{Hangul}\p{Han}0-9\p{P}\p{Zs}\r\n\t\p{Sc}+=]*$",
        r"^[\p{Latin}\u0300-\u036f0-9\p{P}\p{Zs}\r\n\t\p{Sc}+=]*$",
    ].iter().map(|pattern| Regex::new(pattern).unwrap()).collect());
    let script = &scripts[match target { "el" => 0, "zh-Hans" => 1, "ja" => 2, "ko" => 3, _ => 4 }];
    let mut combined = String::new();
    for cue in cues {
        ensure!(has_visible_text(&cue.text), "invalid_segments: blank_text cue {}", cue.id);
        let text = visible_text(&cue.text);
        let letters: Vec<_> = text.chars().filter(|c| c.is_alphabetic()).collect();
        let native = letters.iter().filter(|&&c| match target {
            "el" => matches!(c as u32, 0x370..=0x3ff | 0x1f00..=0x1fff),
            "zh-Hans" => matches!(c as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0x20000..=0x323af),
            "ja" => matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff | 0xff66..=0xff9d | 0x20000..=0x323af),
            "ko" => matches!(c as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af),
            _ => matches!(c as u32, 0x41..=0x7a | 0xc0..=0x24f | 0x1e00..=0x1eff),
        }).count();
        // Common technical acronyms have no language. Do not exempt arbitrary
        // uppercase dialogue: HELLO and GEIA SOU must still fail for Greek.
        let acronym = matches!(text.trim().trim_matches(|c: char| !c.is_alphanumeric()),
            "NASA" | "FBI" | "CIA" | "BBC" | "CNN" | "USB" | "DVD" | "GPS" | "TV" | "CPU" | "GPU" | "DNA" | "RNA" | "SMS");
        // A standalone technical acronym is permitted, but its surrounding
        // punctuation must still pass the character check (no hidden emoji).
        let checked = if acronym { text.replace(|c: char| c.is_ascii_uppercase(), "") } else { text.clone() };
        ensure!(script.is_match(&checked),
            "invalid_segments: translation_language expected {target}, foreign character or symbol in cue {}", cue.id);
        let minimum_native_percent = if matches!(target, "zh-Hans" | "ja" | "ko") { 50 } else { 75 };
        ensure!(letters.is_empty() || acronym || native * 100 >= letters.len() * minimum_native_percent,
            "invalid_segments: translation_language expected {target}, wrong script in cue {}", cue.id);
        check_detected_language(&text, letters.len(), expected, target)?;
        combined.push_str(&text);
        combined.push(' ');
    }
    let letters = combined.chars().filter(|c| c.is_alphabetic()).count();
    check_detected_language(&combined, letters, expected, target)
}
fn check_detected_language(text: &str, letters: usize, expected: &str, target: &str) -> Result<()> {
    // Short names and interjections cannot be classified reliably. Native-script
    // checks above still apply to every cue, including these short utterances.
    if letters >= 40 {
        if let Some(info) = whatlang::detect(text) {
            ensure!(!info.is_reliable() || info.lang().code() == expected,
                "invalid_segments: translation_language expected {target}, detected {}", info.lang().code());
        }
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
            escape_text(&wrap(&plain_text(&c.text)))
        ));
    }
    out
}
fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
/// Skip only an existing, canonical plain subtitle, never a styled source whose
/// embedded font sizes or WebVTT stylesheet could reproduce the reported bug.
pub fn canonical_file(path: &Path, cues: &[Cue], format: &str) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some(format)
        && read_text(path).is_ok_and(|text| text.trim_start_matches('\u{feff}').replace("\r\n", "\n") == render(cues, format))
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
        // Musical-note delimiters describe singing, not dialogue. Providers
        // commonly copy them from ASS captions despite the plain-text contract.
        // Remove only these known delimiters; never discard foreign letters.
        let text = plain_text(&text.replace(['♪', '♫', '♬', '♩'], ""));
        ensure!(
            has_visible_text(&text) && map.insert(id, text).is_none(),
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
    fn font_sizes_multiline_tags_and_overrides_never_reach_the_player() {
        for text in [
            "<font face=\"sans-serif\" size=\"71\">Καλημέρα.</font>",
            "<font face=\"sans-serif\"\nsize=\"71\">Καλημέρα.</font>",
            "&lt;font size=\"999\"&gt;Καλημέρα.&lt;/font&gt;",
            "&amp;lt;font size=\"999\"&amp;gt;Καλημέρα.&amp;lt;/font&amp;gt;",
            "{\\fs120\\fscx300\\pos(0,0)}Καλημέρα.",
            "<style>::cue { font-size: 999px }</style><b>Καλημέρα.</b>",
        ] {
            assert_eq!(plain_text(text), "Καλημέρα.");
            for format in ["srt", "vtt"] {
                let result = render(&[sample(text)], format);
                assert!(!result.contains("font"), "{result}");
                assert!(!result.contains('\\'), "{result}");
                let cues = parse(&result, format).unwrap();
                assert_eq!(cues.len(), 1);
                assert_eq!(cues[0].text, "Καλημέρα.");
                assert_eq!((cues[0].start, cues[0].end), (1., 2.));
            }
        }
    }
    #[test]
    fn plain_text_preserves_dialogue_entities_and_line_breaks() {
        assert_eq!(plain_text("<i>Γεια</i><br/>κόσμε.\\NΔεύτερη\\hγραμμή."), "Γεια\nκόσμε.\nΔεύτερη γραμμή.");
        assert_eq!(plain_text("{\\p1}m 0 0 l 100 100{\\p0}Γεια σου."), "Γεια σου.");
        let text = "2 < 3 & 5 > 4\n\n   Κείμενο &quot;εδώ&quot;.";
        let result = render(&[sample(text)], "srt");
        assert!(result.contains("2 &lt; 3 &amp; 5 &gt; 4"));
        let cues = parse(&result, "srt").unwrap();
        assert_eq!(plain_text(&cues[0].text), "2 < 3 & 5 > 4\nΚείμενο \"εδώ\".");
        assert_eq!(render(&cues, "srt"), result);
    }
    #[test]
    fn translated_text_is_cleaned_before_caching_and_style_only_replies_fail() {
        let source = [sample("Hello.")];
        let value = serde_json::json!({"segments":[{"id":1,"text":"<font\nsize=\"71\">Γεια σου.</font>"}]});
        let cues = translation_result(&value, &source).unwrap();
        assert_eq!(cues[0].text, "Γεια σου.");
        assert_eq!((cues[0].id, cues[0].start, cues[0].end), (1, 1., 2.));
        let empty = serde_json::json!({"segments":[{"id":1,"text":"<style>font-size: 99px</style>{\\fs120}<font size=\"71\"></font>"}]});
        assert!(translation_result(&empty, &source).is_err());
    }
    #[test]
    fn styled_existing_subtitles_must_be_exported_again_even_in_the_target_language() {
        let path = std::env::temp_dir().join(format!("{}.srt", uuid::Uuid::new_v4()));
        let text = "1\n00:00:01,000 --> 00:00:02,000\n<font\nsize=\"71\">Καλημέρα.</font>\n\n";
        fs::write(&path, text).unwrap();
        let cues = read(&path).unwrap();
        assert!(!canonical_file(&path, &cues, "srt"));
        fs::write(&path, render(&cues, "srt")).unwrap();
        assert!(canonical_file(&path, &read(&path).unwrap(), "srt"));
        assert!(!canonical_file(&path, &cues, "vtt"));
        fs::remove_file(path).unwrap();
    }
    fn sample(text: &str) -> Cue {
        Cue { id: 1, start: 1., end: 2., text: text.into() }
    }
    #[test]
    fn invisible_or_markup_only_text_cannot_be_saved() {
        for text in ["", " \n\t", "\u{200b}\u{feff}\u{2060}", "<i></i>", "<b> &nbsp; </b>", "{\\i1}", "\u{0301}", "&#8203;", "&#x200b;", "&ZeroWidthSpace;", "\u{2800}\u{3164}"] {
            let cues = vec![sample(text)];
            assert!(validate(&cues, 10.).is_err(), "accepted {text:?}");
            assert!(translation_result(&serde_json::json!({"segments":[{"id":1,"text":text}]}), &cues).is_err());
        }
        assert!(validate(&[sample("<i>Γεια σου!</i>")], 10.).is_ok());
    }
    #[test]
    fn target_script_is_checked_for_every_cue() {
        for (target, text) in [("el", "Γεια σου, κόσμε!"), ("zh-Hans", "你好，世界！"), ("ja", "こんにちは、世界！"), ("ko", "안녕하세요!"), ("en", "Hello!"), ("es", "¡Hola!"), ("fr", "Bonjour !"), ("de", "Guten Tag!"), ("pt", "Olá!"), ("it", "Ciao!")] {
            validate_language(&[sample(text)], target).unwrap();
        }
        for text in ["Geia sou", "GEIA SOU", "HELLO", "Ti kaneis;", "Hello there!", "Привет", "Γεια σου! This is untranslated dialogue.", "Πώς είσαι σήμερα; How are you today?"] {
            assert!(validate_language(&[sample(text)], "el").is_err(), "accepted {text}");
        }
        let mut mixed = vec![sample("Αυτή είναι μια κανονική ελληνική πρόταση.")];
        let mut foreign = sample("Ti kaneis;");
        foreign.id = 2;
        mixed.push(foreign);
        assert!(validate_language(&mixed, "el").is_err());
        validate_language(&[sample("NASA"), sample("123"), sample("...")], "el").unwrap();
    }
    #[test]
    fn reliable_wrong_language_is_rejected_even_in_the_same_script() {
        let english = "This is an English sentence about the people who live in this house. They are waiting for their friends to arrive because they want to have dinner together.";
        validate_language(&[sample(english)], "en").unwrap();
        for target in ["es", "fr", "de", "pt", "it"] {
            assert!(validate_language(&[sample(english)], target).is_err(), "accepted English for {target}");
        }
    }
    #[test]
    fn isolated_foreign_characters_and_icons_are_rejected() {
        for (target, text) in [
            ("el", "Αυτή είναι μια ελληνική πρόταση ع."),
            ("el", "Αυτή είναι μια ελληνική πρόταση अ."),
            ("el", "Αυτή είναι μια ελληνική πρόταση 😀."),
            ("el", "NASA😀"), ("en", "Hello अ"), ("fr", "Bonjour ع"),
            ("ja", "こんにちは ع"), ("ko", "안녕하세요 अ"),
            ("zh-Hans", "你好 😀"), ("el", "Ελληνικά A"),
        ] {
            assert!(validate_language(&[sample(text)], target).is_err(), "accepted {text}");
        }
        for (target, text) in [("el", "Καλημέρα! 20€"), ("fr", "Cafe\u{0301}, 20 €"), ("ja", "コーヒーです。"), ("ko", "안녕하세요! 123")] {
            validate_language(&[sample(text)], target).unwrap();
        }
    }
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
