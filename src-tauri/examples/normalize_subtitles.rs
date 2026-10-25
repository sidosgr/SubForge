// Use the production subtitle formatter to repair an existing sidecar without
// making translation requests. Always writes a new file, preserving the input.
#[path = "../src/subtitles.rs"]
#[allow(dead_code)]
mod subtitles;
#[path = "../src/types.rs"]
#[allow(dead_code)]
mod types;

fn main() -> anyhow::Result<()> {
    use anyhow::ensure;
    use std::{fs::OpenOptions, io::Write, path::PathBuf};
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(args.len() == 2, "usage: normalize_subtitles INPUT OUTPUT");
    let input = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);
    let format = output.extension().and_then(|s| s.to_str()).unwrap_or("");
    ensure!(matches!(format, "srt" | "vtt"), "output must be SRT or WebVTT");
    let source = subtitles::read(&input)?;
    let text = subtitles::render(&source, format);
    let rendered = subtitles::parse(&text, format)?;
    ensure!(source.len() == rendered.len(), "cue count changed");
    for (before, after) in source.iter().zip(&rendered) {
        ensure!((before.start - after.start).abs() <= 0.00051
            && (before.end - after.end).abs() <= 0.00051, "cue timing changed");
        let dialogue = |s: &str| subtitles::plain_text(s).chars().filter(|c| !c.is_whitespace()).collect::<String>();
        ensure!(dialogue(&before.text) == dialogue(&after.text), "dialogue changed in cue {}", before.id);
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(&output)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    println!("Repaired {} cues; dialogue and timestamps preserved. {}", rendered.len(), output.display());
    Ok(())
}
