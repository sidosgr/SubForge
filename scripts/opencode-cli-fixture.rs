// Test-only executable, used exclusively in an isolated SubForge data directory.
use std::{env, fs, io::Write, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};
#[link(name = "kernel32")]
extern "system" { fn GetConsoleWindow() -> *mut std::ffi::c_void; }
fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let root = PathBuf::from(env::var("SUBFORGE_DATA_DIR").unwrap());
    let model = args.windows(2).find(|a| a[0] == "--model").map(|a| a[1].as_str()).unwrap_or("");
    let config = env::var("OPENCODE_CONFIG_CONTENT").unwrap_or_default();
    let key = env::var("OPENCODE_API_KEY").unwrap_or_default();
    let tick = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let mut log = fs::OpenOptions::new().create(true).append(true).open(root.join("cli-fixture.log")).unwrap();
    writeln!(log, "{}\t{}\t{}\t{}\t{}\t{}", args.first().map(String::as_str).unwrap_or(""), model,
        key == "synthetic-subforge-012-key", config.contains("{env:OPENCODE_API_KEY}"),
        unsafe { GetConsoleWindow().is_null() }, tick).unwrap();
    if args.first().is_some_and(|a| a == "models") {
        assert!(args.iter().any(|a| a == "--refresh"));
        println!("opencode/space-bunny-free\nopencode/muse-spark-1.3-contributor-free\nopencode/mimo-v2.6-flash-free\nopenrouter/unrelated:free");
        return;
    }
    assert_eq!(args.first().map(String::as_str), Some("run"));
    let file_index = args.iter().position(|a| a == "--file").unwrap();
    assert_eq!(file_index + 2, args.len(), "file argument must be last");
    let prompt = fs::read_to_string(&args[file_index + 1]).unwrap();
    assert!(prompt.contains("Input segments:"));
    let task_config = fs::read_to_string("opencode.json").unwrap();
    assert!(task_config.contains(model));
    assert!(task_config.contains("\"permission\":\"ask\""));
    assert!(args.iter().any(|a| a == "--no-auto"));
    assert!(!task_config.contains("synthetic-subforge-012-key"));
    let mode = fs::read_to_string(root.join("fixture-mode.txt")).unwrap_or_default();
    let fail = match mode.trim() {
        "fallback" => model.ends_with("space-bunny-free"),
        "retry" => model.ends_with("space-bunny-free") && !root.join("attempted").exists(),
        "fail-all" => true,
        _ => false,
    };
    if fail {
        fs::write(root.join("attempted"), b"fixture").unwrap();
        let code = if mode.trim() == "retry" { 503 } else { 403 };
        // Deliberately exit with zero: real headless CLIs may report errors this way.
        println!("{{\"type\":\"error\",\"error\":{{\"data\":{{\"statusCode\":{code},\"message\":\"fixture failure\"}}}}}}");
        return;
    }
    if mode.trim() == "malformed" && model.ends_with("space-bunny-free") {
        println!("{}", r#"{"type":"text","part":{"text":"not JSON"}}"#);
        return;
    }
    let second = if prompt.contains("\"id\":2") { r#",{\"id\":2,\"text\":\"Δεύτερη δοκιμή.\"}"# } else { "" };
    println!(r#"{{"type":"text","part":{{"text":"{{\"language\":\"el\",\"segments\":[{{\"id\":1,\"text\":\"Γεια σου.\"}}{second}]}}"}}}}"#);
}
