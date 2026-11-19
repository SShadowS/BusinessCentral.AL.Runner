//! A `al-runner serve` child process and the app helpers the serve tests share.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct Server {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub stdout: BufReader<ChildStdout>,
}

impl Server {
    pub fn start(extra: &[&str]) -> Server {
        Server::start_with(extra, &[])
    }

    pub fn start_with(extra: &[&str], env: &[(&str, &str)]) -> Server {
        Server::start_with_stderr(extra, env, Stdio::null())
    }

    pub fn start_with_stderr(extra: &[&str], env: &[(&str, &str)], stderr: Stdio) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_al-runner"))
            .arg("serve")
            .args(extra)
            .env_remove("ALRUNNER_SEED")
            .env_remove("AL_RUNNER_SEED")
            .envs(env.iter().copied())
            .env("AL_CHECK_THREADS", "2")
            .env("AL_LOWER_THREADS", "2")
            .env("AL_TEST_THREADS", "2")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()
            .expect("start al-runner serve");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server { child, stdin, stdout }
    }

    pub fn send(&mut self, v: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{v}").unwrap();
        stdin.flush().unwrap();
    }

    /// The next line, which must be one JSON object.
    pub fn line(&mut self) -> Value {
        let mut s = String::new();
        assert!(self.stdout.read_line(&mut s).unwrap() > 0, "the server closed stdout");
        serde_json::from_str(&s).unwrap_or_else(|e| panic!("stdout holds more than protocol: {e}: {s}"))
    }

    /// Lines up to and including the first one of `type` summary or an error.
    pub fn until_summary(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        loop {
            let l = self.line();
            let done = l["type"] == "summary" || l.get("error").is_some();
            out.push(l);
            if done {
                return out;
            }
        }
    }
}

pub fn write(dir: &Path, rel: &str, content: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

pub fn app(root: &Path, name: &str, al: &str) -> PathBuf {
    let dir = root.join(name);
    write(
        &dir,
        "app.json",
        &format!(
            r#"{{"id":"11111111-0000-0000-0000-0000000000{:02}","name":"{name}","publisher":"Test","version":"1.0.0.0","dependencies":[]}}"#,
            name.len() % 100
        ),
    );
    write(&dir, "src/T.al", al);
    dir
}
