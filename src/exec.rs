//! Runs commands on a background thread.
//!
//! Commands that need administrator rights go through one privileged shell
//! started with `pkexec /bin/sh`, so the person sees a single password prompt
//! per app session. In demo mode a simulator answers instead.

use crate::demo::Demo;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Root {
    /// Run as the person.
    No,
    /// Needs administrator rights (asks once if not unlocked yet).
    Yes,
    /// Use administrator rights only if already unlocked.
    IfUnlocked,
}

#[derive(Clone, Debug)]
pub struct Step {
    pub argv: Vec<String>,
    pub root: Root,
    /// What the activity log shows (secrets removed).
    pub display: String,
    /// Exit codes that count as success.
    pub ok: Vec<i32>,
}

impl Step {
    pub fn new(argv: &[&str], root: Root) -> Self {
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        let display = argv.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ");
        Step { argv, root, display, ok: vec![0] }
    }
    pub fn owned(argv: Vec<String>, root: Root) -> Self {
        let display = argv.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ");
        Step { argv, root, display, ok: vec![0] }
    }
    pub fn ok_codes(mut self, ok: &[i32]) -> Self {
        self.ok = ok.to_vec();
        self
    }
    pub fn redact(mut self, secret: &str) -> Self {
        if !secret.is_empty() {
            self.display = self.display.replace(&shell_quote(secret), "'••••••'");
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobKind {
    Refresh,
    Discover { portal: String },
    Action,
    Unlock,
    Install,
}

#[derive(Clone, Debug)]
pub struct Job {
    pub kind: JobKind,
    pub label: String,
    pub steps: Vec<Step>,
    /// Keep going after a failed step.
    pub keep_going: bool,
    /// Don't write successful steps to the activity log.
    pub quiet: bool,
}

#[derive(Clone, Debug)]
pub struct StepResult {
    pub display: String,
    pub code: i32,
    pub output: String,
    pub elevated: bool,
    pub ok: bool,
}

#[derive(Clone, Debug)]
pub struct JobResult {
    pub job: Job,
    pub results: Vec<StepResult>,
    pub ok: bool,
    pub took: Duration,
}

impl JobResult {
    pub fn out(&self, i: usize) -> Option<&StepResult> {
        self.results.get(i)
    }
    pub fn first_error(&self) -> Option<&StepResult> {
        self.results.iter().find(|r| !r.ok)
    }
}

pub struct Runner {
    tx: Sender<Job>,
    pub rx: Receiver<JobResult>,
    pub unlocked: Arc<AtomicBool>,
}

impl Runner {
    pub fn new(demo: bool, ctx: egui::Context) -> Self {
        let (tx, job_rx) = channel::<Job>();
        let (res_tx, rx) = channel::<JobResult>();
        let unlocked = Arc::new(AtomicBool::new(is_root()));
        let flag = unlocked.clone();
        std::thread::spawn(move || {
            let mut worker = Worker { helper: None, demo: demo.then(Demo::new), unlocked: flag };
            while let Ok(job) = job_rx.recv() {
                let start = Instant::now();
                let mut results = Vec::new();
                let mut ok = true;
                for step in &job.steps {
                    let r = worker.run(step);
                    let failed = !r.ok;
                    results.push(r);
                    if failed {
                        ok = false;
                        if !job.keep_going {
                            break;
                        }
                    }
                }
                let _ = res_tx.send(JobResult { job, results, ok, took: start.elapsed() });
                ctx.request_repaint();
            }
            if let Some(mut h) = worker.helper.take() {
                h.close();
            }
        });
        Runner { tx, rx, unlocked }
    }

    pub fn send(&self, job: Job) {
        let _ = self.tx.send(job);
    }

    pub fn is_unlocked(&self) -> bool {
        self.unlocked.load(Ordering::Relaxed)
    }
}

struct Worker {
    helper: Option<Helper>,
    demo: Option<Demo>,
    unlocked: Arc<AtomicBool>,
}

impl Worker {
    fn run(&mut self, step: &Step) -> StepResult {
        let unlocked = self.unlocked.load(Ordering::Relaxed);
        let elevate = match step.root {
            Root::No => false,
            Root::Yes => true,
            Root::IfUnlocked => unlocked,
        };
        let (code, output) = if let Some(demo) = self.demo.as_mut() {
            std::thread::sleep(Duration::from_millis(if elevate { 60 } else { 15 }));
            if elevate {
                self.unlocked.store(true, Ordering::Relaxed);
            }
            demo.run(&step.argv)
        } else if !elevate || is_root() {
            run_plain(&step.argv)
        } else {
            self.run_elevated(&step.argv)
        };
        StepResult {
            display: step.display.clone(),
            ok: step.ok.contains(&code),
            code,
            output,
            elevated: elevate,
        }
    }

    fn run_elevated(&mut self, argv: &[String]) -> (i32, String) {
        let alive = self.helper.as_mut().map(|h| h.alive()).unwrap_or(false);
        if !alive {
            self.helper = None;
            self.unlocked.store(false, Ordering::Relaxed);
            match Helper::start() {
                Ok(h) => {
                    self.helper = Some(h);
                    self.unlocked.store(true, Ordering::Relaxed);
                }
                Err(e) => return (126, e),
            }
        }
        let h = self.helper.as_mut().expect("helper");
        match h.run(argv) {
            Ok(r) => r,
            Err(e) => {
                self.helper = None;
                self.unlocked.store(false, Ordering::Relaxed);
                (125, format!("The administrator helper stopped unexpectedly: {e}"))
            }
        }
    }
}

fn run_plain(argv: &[String]) -> (i32, String) {
    let Some((prog, args)) = argv.split_first() else { return (0, String::new()) };
    match Command::new(prog).args(args).env("LC_ALL", "C").stdin(Stdio::null()).output() {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            let e = String::from_utf8_lossy(&o.stderr);
            if !e.trim().is_empty() {
                if !s.is_empty() && !s.ends_with('\n') {
                    s.push('\n');
                }
                s.push_str(&e);
            }
            (o.status.code().unwrap_or(-1), s)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (127, format!("{prog}: command not found")),
        Err(e) => (126, format!("{prog}: {e}")),
    }
}

const END: &str = "__ISCSI_MANAGER_END__";
const READY: &str = "__ISCSI_MANAGER_READY__";

struct Helper {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
}

impl Helper {
    fn start() -> Result<Helper, String> {
        let mut child = Command::new("pkexec")
            .arg("/bin/sh")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Couldn't ask for administrator rights (pkexec): {e}"))?;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        let mut out = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        writeln!(stdin, "echo {READY}").map_err(|e| e.to_string())?;
        stdin.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        loop {
            line.clear();
            match out.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
                    return Err(match code {
                        126 => "Administrator permission was not given (the password prompt was cancelled).".into(),
                        127 => "Administrator permission was refused or no password prompt is available.".into(),
                        c => format!("Couldn't get administrator rights (pkexec exit code {c})."),
                    });
                }
                Ok(_) if line.trim() == READY => break,
                Ok(_) => {}
            }
        }
        Ok(Helper { child, stdin, out })
    }

    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn run(&mut self, argv: &[String]) -> std::io::Result<(i32, String)> {
        let cmd = argv.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ");
        writeln!(self.stdin, "LC_ALL=C {cmd} </dev/null 2>&1; printf '\\n{END}%s\\n' \"$?\"")?;
        self.stdin.flush()?;
        let mut output = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if self.out.read_line(&mut line)? == 0 {
                return Err(std::io::Error::other("helper closed"));
            }
            if let Some(code) = line.trim_end().strip_prefix(END) {
                let code = code.trim().parse().unwrap_or(-1);
                if output.ends_with('\n') {
                    output.pop();
                }
                return Ok((code, output));
            }
            output.push_str(&line);
        }
    }

    fn close(&mut self) {
        let _ = writeln!(self.stdin, "exit");
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

pub fn shell_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:=,@[]+".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(2).map(|e| e == "0"))
        })
        .unwrap_or(false)
}

pub fn find_program(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let found = path
        .split(':')
        .chain(["/usr/sbin", "/sbin", "/usr/bin", "/bin"])
        .map(|d| std::path::Path::new(d).join(name))
        .find(|p| p.is_file());
    found
}
