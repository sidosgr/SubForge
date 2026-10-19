// Test-only executable: synthetic replies inside an isolated application profile.
use serde_json::{json,Value};
use std::{env,fs,io::Write,path::PathBuf};
#[link(name="kernel32")]
extern "system" {fn GetConsoleWindow()->*mut std::ffi::c_void;}
fn main(){
 let args:Vec<String>=env::args().skip(1).collect();let root=PathBuf::from(env::var("SUBFORGE_DATA_DIR").unwrap());
 if let Ok(delay)=fs::read_to_string(root.join("reply-delay-ms.txt")){std::thread::sleep(std::time::Duration::from_millis(delay.trim().parse().unwrap()));}
 let (kind,prompt)=if args.first().is_some_and(|a|a=="run"){
  assert!(args.windows(2).any(|a|a==["--agent","subforge-translator"]));assert!(args.iter().any(|a|a=="--pure"));assert!(args.iter().any(|a|a=="--no-auto"));
  let file=args.windows(2).find(|a|a[0]=="--file").unwrap();("opencode",fs::read_to_string(&file[1]).unwrap())
 }else if let Some(index)=args.iter().position(|a|a=="-p"){
  let schema=args.windows(2).find(|a|a[0]=="--json-schema").unwrap();let value:Value=serde_json::from_str(&schema[1]).unwrap();assert!(value["required"].as_array().unwrap().contains(&json!("segments")));
  assert!(args.windows(2).any(|a|a==["--mode","plan"]));assert!(args.iter().any(|a|a=="--sandbox"));("antigravity",args[index+1].clone())
 }else{
  assert!(args.iter().any(|a|a=="--no-tools"));assert!(args.iter().any(|a|a=="--no-session"));let file=args.last().unwrap().strip_prefix('@').unwrap();("pi",fs::read_to_string(file).unwrap())
 };
 let cues:Vec<Value>=serde_json::from_str(prompt.split("Input segments: ").nth(1).unwrap()).unwrap();
 let mode=fs::read_to_string(root.join("reply-mode.txt")).unwrap_or_default();
 let mut log=fs::OpenOptions::new().create(true).append(true).open(root.join("reply-fixture.jsonl")).unwrap();
 writeln!(log,"{}",json!({"kind":kind,"ids":cues.iter().map(|c|&c["id"]).collect::<Vec<_>>(),"hidden":unsafe{GetConsoleWindow().is_null()}})).unwrap();
 if mode.trim()=="quota"&&kind=="antigravity"{println!("{}",json!({"status":"ERROR","error":"Error 429: synthetic quota"}));return;}
 if mode.trim()=="empty"&&cues.len()>1{return;}
 let invalid=mode.trim()=="split"&&cues.len()>2;
 let value=if mode.trim()=="missing"&&cues.len()>1{json!({"language":"el","segments":[]})}else{json!({"language":"el","segments":cues.iter().map(|c|json!({"id":c["id"],"text":format!("Ελληνικά {}: λόγια {{με}} εισαγωγικά \"και\"\nνέα γραμμή.",c["id"])})).collect::<Vec<_>>()})};
 let text=if invalid{"incomplete {\"segments\":[".into()}else{value.to_string()};
 match kind{
 "opencode"=>{if mode.trim()=="repeated"{println!("{}",json!({"type":"text","part":{"id":"old","text":"Earlier answer {\"segments\":[]}"}}));println!("{}",json!({"type":"text","part":{"id":"copy","text":text}}));}println!("{}",json!({"type":"text","part":{"id":"final","text":text}}));},
 "antigravity"=>{if invalid{println!("{}",json!({"status":"SUCCESS","response":text}));}else{println!("{}",json!({"status":"SUCCESS","response":"preamble is deliberately not JSON","structured_output":value}));}},
 _=>{if mode.trim()=="repeated"{println!("Earlier answer {{\"segments\":[]}}");}println!("{text}");}
 }
}
