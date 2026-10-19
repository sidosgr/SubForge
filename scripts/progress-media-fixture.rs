// Owned test-only FFmpeg stand-in. Probe and model verification remain real.
use std::{env,fs,path::PathBuf,time::Duration};
fn main(){
 let args:Vec<String>=env::args().skip(1).collect();
 let root=PathBuf::from(env::var("SUBFORGE_DATA_DIR").unwrap());
 if args.iter().any(|s|s.contains("whisper=model=")) {
   std::thread::sleep(Duration::from_secs(3));
   fs::write("local.srt","1\n00:00:01,000 --> 00:00:01,000\nSynthetic zero-duration cue\n\n2\n00:00:02,000 --> 00:00:03,000\nSynthetic second cue\n\n3\n00:00:04,000 --> 00:00:05,000\nSynthetic third cue\n").unwrap();
   let marker=root.join("recognition-calls.txt");let count=fs::read_to_string(&marker).ok().and_then(|s|s.parse::<usize>().ok()).unwrap_or(0)+1;fs::write(marker,count.to_string()).unwrap();
 } else { fs::copy(root.join("fixture.wav"),args.last().unwrap()).unwrap(); }
}
