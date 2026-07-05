// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub struct TempProjectRoot {
    path: PathBuf,
}

impl Deref for TempProjectRoot {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl Drop for TempProjectRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[allow(dead_code)]
pub fn run(args: &[&str]) -> intlify_cli::CliRunResult {
    run_in(args, Path::new("."))
}

pub fn run_in(args: &[&str], cwd: &Path) -> intlify_cli::CliRunResult {
    intlify_cli::run(args.iter().copied(), cwd)
}

#[allow(dead_code)]
pub fn run_stdin(args: &[&str], cwd: &Path, stdin: &str) -> intlify_cli::CliRunResult {
    intlify_cli::run_with_stdin(args.iter().copied(), cwd, stdin.as_bytes())
}

pub fn json_stdout(result: &intlify_cli::CliRunResult) -> Value {
    serde_json::from_str(result.stdout.trim_end()).expect("stdout should be JSON")
}

pub fn temp_project_root(name: &str) -> TempProjectRoot {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "intlify-cli-{name}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(path.join(".git")).expect("temp project git marker should be created");
    TempProjectRoot { path }
}

#[allow(dead_code)]
pub fn write(path: &Path, source: &str) {
    fs::create_dir_all(path.parent().expect("fixture parent"))
        .expect("fixture parent should be created");
    fs::write(path, source).expect("fixture should be written");
}

#[allow(dead_code)]
pub fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("fixture should be readable")
}
