// Synthetic media/recognition stand-in for chunk-8 recovery regression only.
use std::{env,fs,path::PathBuf,io::Write};
fn main(){
 let args:Vec<String>=env::args().skip(1).collect();let root=PathBuf::from(env::var("SUBFORGE_DATA_DIR").unwrap());
 if args.iter().any(|s|s.contains("whisper=model=")) {
  let cwd=env::current_dir().unwrap();let part:usize=cwd.file_name().unwrap().to_str().unwrap().strip_prefix("chunk-").unwrap().parse().unwrap();
  writeln!(fs::OpenOptions::new().create(true).append(true).open(root.join("recognition-parts.txt")).unwrap(),"{part}").unwrap();
  let text=if part==7 {let mut text=fs::read_to_string(root.join("timing.srt")).unwrap();if fs::read_to_string(root.join("boundary-mode.txt")).unwrap().trim()=="outside"{text=text.replace("00:04:59,520 --> 00:05:09,520","00:05:00,520 --> 00:05:09,520");}text}else{"1\n00:00:00,000 --> 00:00:01,000\nSynthetic dialogue\n".into()};
  fs::write("local.srt",text).unwrap();
 }else{fs::copy(root.join("fixture.wav"),args.last().unwrap()).unwrap();}
}
