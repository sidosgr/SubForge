// Test-only sign-in terminal. No browser, credentials, or real terms are involved.
use std::io::{Read, Write};
fn main() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(which: u32) -> isize;
        fn GetConsoleMode(handle: isize, mode: *mut u32) -> i32;
        fn SetConsoleMode(handle: isize, mode: u32) -> i32;
    }
    unsafe {
        let input = GetStdHandle(-10i32 as u32);
        let mut mode = 0;
        assert_ne!(GetConsoleMode(input, &mut mode), 0);
        assert_ne!(SetConsoleMode(input, (mode & !6) | 0x0200), 0);
        let output = GetStdHandle(-11i32 as u32);
        assert_ne!(GetConsoleMode(output, &mut mode), 0);
        assert_ne!(SetConsoleMode(output, mode | 4), 0);
    }
    let root = std::path::PathBuf::from(std::env::var("SUBFORGE_DATA_DIR").unwrap());
    let mut stdout = std::io::stdout();
    let mut stdin = std::io::stdin();
    let mut receive = |expected: &[u8], stage: &str| {
        let mut buf = vec![0; expected.len()]; stdin.read_exact(&mut buf).unwrap();
        assert_eq!(buf, expected, "unexpected {stage} input");
        let mut log = std::fs::OpenOptions::new().create(true).append(true).open(root.join("signin-fixture.log")).unwrap();
        writeln!(log, "{stage}").unwrap();
    };
    stdout.write_all(b"\x1b[").unwrap(); stdout.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    stdout.write_all(b"6n").unwrap(); stdout.flush().unwrap();
    receive(b"\x1b[1;1R", "terminal-initialized");
    writeln!(stdout, "Choose your color scheme").unwrap(); stdout.flush().unwrap();
    receive(b"\r", "theme-selected");
    writeln!(stdout, "Terms of Service & Data Use\n[x] Share interactions\nhttps://antigravity.google/terms\nhttps://policies.google.com/privacy").unwrap(); stdout.flush().unwrap();
    receive(b"\r", "sharing-disabled");
    receive(b"\x1b[B\t\r", "explicit-setup-confirmation");
    if root.join("signin-connected.txt").exists() {
        writeln!(stdout, "Antigravity CLI account@example.test Gemini 3.8 Flash").unwrap(); stdout.flush().unwrap();
        receive(b"\x15/logout\r", "explicit-account-switch");
    }
    writeln!(stdout, "authentication required. Complete sign-in in your browser.").unwrap(); stdout.flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(60));
}
