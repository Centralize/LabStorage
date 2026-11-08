use crate::exec::{find_program, Job, JobKind, JobResult, Root, Runner, Step, StepResult};
use crate::model::*;
use crate::ui::*;
use egui::{Margin, Stroke};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Connections,
    Targets,
    Discover,
    Computer,
    Activity,
}

impl Page {
    pub fn from_name(s: &str) -> Option<Page> {
        Some(match s.to_ascii_lowercase().as_str() {
            "connections" | "sessions" => Page::Connections,
            "targets" | "nodes" => Page::Targets,
            "discover" | "find" => Page::Discover,
            "computer" | "settings" => Page::Computer,
            "activity" | "log" => Page::Activity,
            _ => return None,
        })
    }
}

#[derive(Clone, Default)]
pub struct DiskInfo {
    pub size: u64,
    pub model: String,
    pub mounts: Vec<String>,
}

pub struct LogEntry {
    pub when: Instant,
    pub took: Duration,
    pub label: String,
    pub ok: bool,
    pub steps: Vec<StepResult>,
}

pub struct ChapEdit {
    pub target: String,
    pub portal: String,
    pub chap: bool,
    pub user: String,
    pub pass: String,
    pub mutual: bool,
    pub user_in: String,
    pub pass_in: String,
    pub connect_after: bool,
}

pub struct Confirm {
    pub title: String,
    pub body: String,
    pub button: String,
    pub job: Job,
}

pub struct Options {
    pub screenshot: Option<PathBuf>,
    pub force_demo: bool,
    pub page: Page,
}

pub struct IscsiApp {
    pub ctx: egui::Context,
    pub runner: Runner,
    pub demo: bool,
    pub force_demo: bool,
    pub iscsiadm: Option<PathBuf>,
    pub page: Page,

    pub sessions: Vec<Session>,
    pub nodes: Vec<Node>,
    pub nodes_error: Option<String>,
    pub nodes_elevated: bool,
    pub services: Vec<ServiceInfo>,
    pub initiator: Option<String>,
    pub disks: HashMap<String, DiskInfo>,
    pub loaded: bool,

    pub discover_addr: String,
    pub discover_port: String,
    pub discovered: Vec<Discovered>,
    pub discover_error: Option<String>,
    pub discover_portal: String,

    pub filter: String,
    pub log: Vec<LogEntry>,
    pub toast: Option<(bool, String, Instant)>,
    pub pending: usize,
    pub refresh_pending: bool,
    pub last_refresh: Instant,
    pub was_unlocked: bool,

    pub chap: Option<ChapEdit>,
    pub confirm: Option<Confirm>,
    pub initiator_edit: Option<String>,

    pub shot: Option<PathBuf>,
    pub frame: u64,
    pub shot_done: Arc<AtomicBool>,
}

pub fn unlock_job() -> Job {
    Job {
        kind: JobKind::Unlock,
        label: "Unlock administrator".into(),
        steps: vec![Step::new(&["true"], Root::Yes)],
        keep_going: false,
        quiet: true,
    }
}

impl IscsiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: Options) -> Self {
        setup_style(&cc.egui_ctx);
        let iscsiadm = find_program("iscsiadm");
        let demo = opts.force_demo || iscsiadm.is_none();
        let runner = Runner::new(demo, cc.egui_ctx.clone());
        let mut app = IscsiApp {
            ctx: cc.egui_ctx.clone(),
            was_unlocked: runner.is_unlocked(),
            runner,
            demo,
            force_demo: opts.force_demo,
            iscsiadm,
            page: opts.page,
            sessions: Vec::new(),
            nodes: Vec::new(),
            nodes_error: None,
            nodes_elevated: false,
            services: Vec::new(),
            initiator: None,
            disks: HashMap::new(),
            loaded: false,
            discover_addr: String::new(),
            discover_port: "3260".into(),
            discovered: Vec::new(),
            discover_error: None,
            discover_portal: String::new(),
            filter: String::new(),
            log: Vec::new(),
            toast: None,
            pending: 0,
            refresh_pending: false,
            last_refresh: Instant::now(),
            chap: None,
            confirm: None,
            initiator_edit: None,
            shot: opts.screenshot,
            frame: 0,
            shot_done: Arc::new(AtomicBool::new(false)),
        };
        app.refresh();
        if app.demo && app.shot.is_some() && app.page == Page::Discover {
            // Show what a search looks like in the checking picture.
            app.discover_addr = "192.168.50.10".into();
            app.discover();
        }
        app
    }

    // ---------------------------------------------------------------- jobs

    pub fn send(&mut self, job: Job) {
        if job.kind == JobKind::Refresh {
            self.refresh_pending = true;
        }
        self.pending += 1;
        self.runner.send(job);
    }

    pub fn refresh(&mut self) {
        if self.refresh_pending {
            return;
        }
        self.last_refresh = Instant::now();
        let steps = vec![
            Step::new(&["iscsiadm", "-m", "session", "-P", "3"], Root::IfUnlocked).ok_codes(&[0, 21]),
            Step::new(&["iscsiadm", "-m", "node", "-o", "show"], Root::IfUnlocked).ok_codes(&[0, 21]),
            Step::new(&["systemctl", "is-active", "iscsid.service", "open-iscsi.service"], Root::No).ok_codes(&[0, 1, 3, 4]),
            Step::new(&["systemctl", "is-enabled", "iscsid.service", "open-iscsi.service"], Root::No).ok_codes(&[0, 1, 4]),
            Step::new(&["cat", "/etc/iscsi/initiatorname.iscsi"], Root::IfUnlocked).ok_codes(&[0, 1]),
        ];
        self.send(Job { kind: JobKind::Refresh, label: "Refresh".into(), steps, keep_going: true, quiet: true });
    }

    pub fn action(&mut self, label: impl Into<String>, steps: Vec<Step>) {
        self.send(Job { kind: JobKind::Action, label: label.into(), steps, keep_going: false, quiet: false });
    }

    pub fn node_step(target: &str, portal: &str, extra: &[&str]) -> Step {
        let mut v = vec!["iscsiadm", "-m", "node", "-T", target, "-p", portal];
        v.extend_from_slice(extra);
        Step::new(&v, Root::Yes)
    }

    pub fn update_step(target: &str, portal: &str, key: &str, val: &str) -> Step {
        Self::node_step(target, portal, &["--op", "update", "-n", key, "-v", val])
    }

    pub fn connect(&mut self, target: &str, portal: &str) {
        // Exit code 15 means "a session already exists": the goal is reached.
        let s = Self::node_step(target, portal, &["--login"]).ok_codes(&[0, 15]);
        self.action(format!("Connect {}", short_name(target)), vec![s]);
    }

    pub fn disconnect(&mut self, target: &str, portal: &str) {
        let s = Self::node_step(target, portal, &["--logout"]);
        self.action(format!("Disconnect {}", short_name(target)), vec![s]);
    }

    pub fn discover(&mut self) {
        let addr = self.discover_addr.trim().to_string();
        let port: u16 = self.discover_port.trim().parse().unwrap_or(3260);
        let portal = fmt_portal(addr.trim_matches(|c| c == '[' || c == ']'), port);
        self.discover_error = None;
        let step = Step::owned(
            vec!["iscsiadm".into(), "-m".into(), "discovery".into(), "-t".into(), "sendtargets".into(), "-p".into(), portal.clone()],
            Root::Yes,
        );
        self.send(Job {
            kind: JobKind::Discover { portal: portal.clone() },
            label: format!("Find targets on {portal}"),
            steps: vec![step],
            keep_going: false,
            quiet: false,
        });
    }

    pub fn install(&mut self) {
        let steps = vec![
            Step::new(&["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", "open-iscsi"], Root::Yes),
            Step::new(&["systemctl", "enable", "--now", "iscsid.service"], Root::Yes).ok_codes(&[0, 1]),
        ];
        // Installing must touch the real system even while sample data is shown.
        if self.demo && !self.force_demo {
            self.runner = Runner::new(false, self.ctx.clone());
            self.demo = false;
            self.pending = 0;
            self.refresh_pending = false;
        }
        self.send(Job { kind: JobKind::Install, label: "Install open-iscsi".into(), steps, keep_going: false, quiet: false });
    }

    fn handle(&mut self, r: JobResult) {
        self.pending = self.pending.saturating_sub(1);
        let unexpected = r.results.iter().any(|s| !s.ok);
        if !r.job.quiet || (unexpected && r.job.kind != JobKind::Refresh) {
            self.log.push(LogEntry { when: Instant::now(), took: r.took, label: r.job.label.clone(), ok: r.ok, steps: r.results.clone() });
            if self.log.len() > 300 {
                self.log.remove(0);
            }
        }
        match r.job.kind.clone() {
            JobKind::Refresh => {
                self.refresh_pending = false;
                self.apply_refresh(&r);
            }
            JobKind::Discover { portal } => {
                self.discover_portal = portal;
                if r.ok {
                    self.discovered = parse_portal_target_lines(&r.results[0].output);
                    let n = self.discovered.len();
                    self.toast(true, format!("Found {n} target{} on {}", if n == 1 { "" } else { "s" }, self.discover_portal));
                } else {
                    self.discovered.clear();
                    let e = r.first_error().map(explain).unwrap_or_default();
                    self.discover_error = Some(e.clone());
                    self.toast(false, e);
                }
                self.refresh();
            }
            JobKind::Install => {
                self.iscsiadm = find_program("iscsiadm");
                if r.ok {
                    self.toast(true, "open-iscsi is installed and its service is running.".into());
                } else {
                    let e = r.first_error().map(explain).unwrap_or_default();
                    self.toast(false, format!("Installing failed: {e}"));
                    if self.iscsiadm.is_none() && !self.force_demo {
                        self.demo = true;
                        self.runner = Runner::new(true, self.ctx.clone());
                        self.refresh_pending = false;
                        self.pending = 0;
                    }
                }
                self.refresh();
            }
            JobKind::Unlock => {
                if r.ok {
                    self.toast(true, "Administrator access unlocked for this session.".into());
                } else {
                    let e = r.first_error().map(explain).unwrap_or_default();
                    self.toast(false, e);
                }
            }
            JobKind::Action => {
                if r.ok && r.results.iter().any(|s| s.code == 15) {
                    self.toast(true, format!("{}: it was already connected", r.job.label));
                } else if r.ok {
                    self.toast(true, format!("{}: done", r.job.label));
                } else {
                    let e = r.first_error().map(explain).unwrap_or_default();
                    self.toast(false, format!("{} failed: {e}", r.job.label));
                }
                self.refresh();
            }
        }
    }

    fn apply_refresh(&mut self, r: &JobResult) {
        self.loaded = true;
        if let Some(s) = r.out(0) {
            self.sessions = if s.code == 0 { parse_sessions(&s.output) } else { Vec::new() };
            // Without administrator rights iscsiadm on Ubuntu 26.04 says "No active
            // sessions" even when some exist (it can't read a root-only file), so the
            // kernel's own list in /sys, readable by everyone, is used as well.
            if !self.demo {
                let from_sys = read_sysfs_sessions(std::path::Path::new("/sys"));
                if self.sessions.is_empty() {
                    self.sessions = from_sys;
                } else {
                    for k in from_sys {
                        if !self.sessions.iter().any(|x| x.sid == k.sid) {
                            self.sessions.push(k);
                        }
                    }
                }
            }
        }
        if let Some(s) = r.out(1) {
            self.nodes_elevated = s.elevated;
            match s.code {
                0 => {
                    self.nodes = parse_node_records(&s.output);
                    self.nodes_error = None;
                }
                21 => {
                    self.nodes.clear();
                    self.nodes_error = None;
                }
                _ => self.nodes_error = Some(explain(s)),
            }
        }
        let lines = |i: usize| -> Vec<String> { r.out(i).map(|s| s.output.lines().map(|l| l.trim().to_string()).collect()).unwrap_or_default() };
        let active = lines(2);
        let enabled = lines(3);
        self.services = ["iscsid.service", "open-iscsi.service"]
            .iter()
            .enumerate()
            .map(|(i, n)| ServiceInfo {
                name: n.to_string(),
                active: active.get(i).cloned().unwrap_or_else(|| "unknown".into()),
                enabled: enabled.get(i).cloned().unwrap_or_else(|| "unknown".into()),
            })
            .collect();
        self.initiator = r.out(4).filter(|s| s.code == 0).and_then(|s| parse_initiator(&s.output));
        // Facts about the attached disks.
        self.disks.clear();
        let names: Vec<String> = self.sessions.iter().flat_map(|s| s.disks.iter().map(|d| d.name.clone())).collect();
        for name in names {
            let info = if self.demo {
                let (size, model, mount) = crate::demo::disk_facts(&name);
                DiskInfo { size, model: model.into(), mounts: if mount.is_empty() { vec![] } else { vec![mount.into()] } }
            } else {
                read_disk_info(&name)
            };
            self.disks.insert(name, info);
        }
    }

    pub fn toast(&mut self, ok: bool, msg: String) {
        self.toast = Some((ok, msg, Instant::now()));
    }

    pub fn session_for(&self, target: &str, portal: &str) -> Option<&Session> {
        let (a, p) = split_portal(portal);
        self.sessions.iter().find(|s| {
            let (sa, sp) = split_portal(&s.portal);
            s.target == target && sa == a && sp.unwrap_or(3260) == p.unwrap_or(3260)
        })
    }

    pub fn open_disks_app(&mut self, dev: &str) {
        if self.demo {
            self.toast(true, format!("Sample mode: the Disks app would open /dev/{dev} here."));
            return;
        }
        match std::process::Command::new("gnome-disks").arg("--block-device").arg(format!("/dev/{dev}")).spawn() {
            Ok(_) => self.toast(true, format!("Opening /dev/{dev} in Disks…")),
            Err(_) => self.toast(false, "The Disks app (gnome-disk-utility) isn't installed.".into()),
        }
    }

    fn screenshot_tick(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        self.frame += 1;
        ctx.request_repaint();
        if self.frame == 120 {
            let done = self.shot_done.clone();
            let c = ctx.clone();
            ctx.request_screenshot(move |img| {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let [w, h] = img.size;
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
                match image::save_buffer(&path, &bytes, w as u32, h as u32, image::ExtendedColorType::Rgba8) {
                    Ok(()) => eprintln!("saved screenshot to {}", path.display()),
                    Err(e) => eprintln!("couldn't save screenshot: {e}"),
                }
                done.store(true, Ordering::SeqCst);
                c.request_repaint();
            });
        }
        if self.shot_done.load(Ordering::SeqCst) || self.frame > 900 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for IscsiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(r) = self.runner.rx.try_recv() {
            self.handle(r);
        }
        let unlocked = self.runner.is_unlocked();
        if unlocked && !self.was_unlocked {
            self.refresh();
        }
        self.was_unlocked = unlocked;
        if self.pending == 0 && self.last_refresh.elapsed() > Duration::from_secs(15) {
            self.refresh();
        }
        ctx.request_repaint_after(Duration::from_secs(1));

        egui::Panel::top("top")
            .frame(egui::Frame::new().fill(SIDE).inner_margin(Margin::symmetric(16, 10)).stroke(Stroke::new(1.0, LINE)))
            .show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(SIDE).inner_margin(Margin::symmetric(16, 6)).stroke(Stroke::new(1.0, LINE)))
            .show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(236.0)
            .frame(egui::Frame::new().fill(SIDE).inner_margin(Margin::symmetric(10, 6)).stroke(Stroke::new(1.0, LINE)))
            .show(ui, |ui| self.nav(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(22, 16)))
            .show(ui, |ui| {
                self.demo_banner(ui);
                // Measured outside the scroll area so content can't widen it frame by frame.
                let width = ui.available_width().min(1040.0);
                egui::ScrollArea::vertical().auto_shrink([true, false]).show(ui, |ui| {
                    ui.set_max_width(width);
                    match self.page {
                        Page::Connections => self.page_connections(ui),
                        Page::Targets => self.page_targets(ui),
                        Page::Discover => self.page_discover(ui),
                        Page::Computer => self.page_computer(ui),
                        Page::Activity => self.page_activity(ui),
                    }
                    ui.add_space(20.0);
                });
            });
        self.modals(&ctx);
        self.screenshot_tick(&ctx);
    }
}

/// Turn a failed step into one plain sentence.
pub fn explain(r: &StepResult) -> String {
    let hint = match r.code {
        4 => Some("Couldn't reach the server. Check the address, the port and the network."),
        5 | 19 => Some("The server refused the connection."),
        6 => Some("The saved settings couldn't be changed (is it still connected?)."),
        8 | 11 => Some("The server didn't answer in time."),
        13 => Some("Permission denied."),
        15 => Some("It's already connected."),
        18 | 20 => Some("The iSCSI service (iscsid) isn't running. Start it on the This computer page."),
        21 => Some("Nothing matched."),
        24 => Some("Login was refused: check the CHAP user name and password, and that the server allows this computer's iSCSI name."),
        _ => None,
    };
    if let Some(h) = hint {
        return h.to_string();
    }
    r.output
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.trim_start_matches("iscsiadm: ").to_string())
        .unwrap_or_else(|| format!("exit code {}", r.code))
}

fn read_disk_info(name: &str) -> DiskInfo {
    let base = format!("/sys/block/{name}");
    let read = |f: &str| std::fs::read_to_string(format!("{base}/{f}")).unwrap_or_default().trim().to_string();
    let size = read("size").parse::<u64>().unwrap_or(0) * 512;
    let model = format!("{} {}", read("device/vendor"), read("device/model")).trim().to_string();
    let dev = format!("/dev/{name}");
    let mounts = std::fs::read_to_string("/proc/mounts")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (d, m) = (it.next()?, it.next()?);
            let rest = d.strip_prefix(&dev)?;
            (rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit())).then(|| m.replace("\\040", " "))
        })
        .collect();
    DiskInfo { size, model, mounts }
}

