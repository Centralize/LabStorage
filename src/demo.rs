//! A pretend storage network that answers like `iscsiadm`, `systemctl` and
//! friends, so the app can be tried safely when open-iscsi isn't installed.

use crate::model::{split_portal, split_tpgt};

struct DNode {
    target: String,
    address: String,
    port: u16,
    startup: String,
    auth: String,
    user: String,
    user_in: String,
    sid: Option<u32>,
    luns: Vec<&'static str>,
    disks: Vec<String>,
}

pub struct Demo {
    nodes: Vec<DNode>,
    next_sid: u32,
    next_disk: u8,
    servers: Vec<(&'static str, Vec<(&'static str, Vec<&'static str>)>)>,
    iscsid: (bool, bool),
    open_iscsi: (bool, bool),
    initiator: String,
}

/// Pretend disk facts for the sample disks: (size in bytes, vendor/model, mount point).
pub fn disk_facts(name: &str) -> (u64, &'static str, &'static str) {
    match name {
        "sdb" => (500 * GIB, "LIO-ORG vm-store-lun0", "/srv/vms"),
        "sdc" => (200 * GIB, "LIO-ORG vm-store-lun1", ""),
        "sdd" => (2048 * GIB, "TrueNAS iSCSI Disk", "/media/lab/media"),
        "sde" => (1024 * GIB, "LIO-ORG backup-lun0", ""),
        _ => (100 * GIB, "LIO-ORG demo-disk", ""),
    }
}
const GIB: u64 = 1024 * 1024 * 1024;

impl Demo {
    pub fn new() -> Self {
        let servers = vec![
            (
                "192.168.50.10",
                vec![
                    ("iqn.2025-01.lab.storage:vm-store", vec!["0", "1"]),
                    ("iqn.2025-01.lab.storage:backup", vec!["0"]),
                    ("iqn.2025-01.lab.storage:scratch", vec!["0"]),
                ],
            ),
            ("192.168.50.20", vec![("iqn.2025-03.lab.nas:media", vec!["0"])]),
        ];
        let mut d = Demo {
            nodes: Vec::new(),
            next_sid: 1,
            next_disk: b'b',
            servers,
            iscsid: (true, true),
            open_iscsi: (true, true),
            initiator: "iqn.2004-10.com.ubuntu:01:5f2c9a1e7b3d".into(),
        };
        d.add_node("iqn.2025-01.lab.storage:vm-store", "192.168.50.10", 3260, vec!["0", "1"]);
        d.add_node("iqn.2025-03.lab.nas:media", "192.168.50.20", 3260, vec!["0"]);
        d.add_node("iqn.2025-01.lab.storage:backup", "192.168.50.10", 3260, vec!["0"]);
        d.nodes[0].startup = "automatic".into();
        d.nodes[0].auth = "CHAP".into();
        d.nodes[0].user = "labhost01".into();
        d.nodes[1].startup = "automatic".into();
        d.login(0);
        d.login(1);
        d
    }

    fn add_node(&mut self, t: &str, a: &str, port: u16, luns: Vec<&'static str>) -> usize {
        if let Some(i) = self.nodes.iter().position(|n| n.target == t && n.address == a && n.port == port) {
            return i;
        }
        self.nodes.push(DNode {
            target: t.into(),
            address: a.into(),
            port,
            startup: "manual".into(),
            auth: "None".into(),
            user: String::new(),
            user_in: String::new(),
            sid: None,
            luns,
            disks: Vec::new(),
        });
        self.nodes.len() - 1
    }

    fn login(&mut self, i: usize) {
        if self.nodes[i].sid.is_none() {
            self.nodes[i].sid = Some(self.next_sid);
            self.next_sid += 1;
            let mut disks = Vec::new();
            for _ in 0..self.nodes[i].luns.len() {
                disks.push(format!("sd{}", self.next_disk as char));
                self.next_disk = if self.next_disk >= b'z' { b'b' } else { self.next_disk + 1 };
            }
            self.nodes[i].disks = disks;
        }
    }

    fn arg<'a>(argv: &'a [String], flags: &[&str]) -> Option<&'a str> {
        argv.iter()
            .position(|a| flags.contains(&a.as_str()))
            .and_then(|i| argv.get(i + 1))
            .map(String::as_str)
    }

    fn has(argv: &[String], flag: &str) -> bool {
        argv.iter().any(|a| a == flag || a.starts_with(&format!("{flag}=")))
    }

    fn find(&self, argv: &[String]) -> Vec<usize> {
        let t = Self::arg(argv, &["-T", "--targetname"]);
        let p = Self::arg(argv, &["-p", "--portal"]).map(|p| {
            let (a, port) = split_portal(&split_tpgt(p).0);
            (a, port.unwrap_or(3260))
        });
        (0..self.nodes.len())
            .filter(|&i| {
                let n = &self.nodes[i];
                t.map_or(true, |t| n.target == t) && p.as_ref().map_or(true, |(a, port)| &n.address == a && n.port == *port)
            })
            .collect()
    }

    fn portal(n: &DNode) -> String {
        crate::model::fmt_portal(&n.address, n.port)
    }

    pub fn run(&mut self, argv: &[String]) -> (i32, String) {
        let Some(prog) = argv.first() else { return (0, String::new()) };
        match prog.as_str() {
            "iscsiadm" => self.iscsiadm(argv),
            "systemctl" => self.systemctl(argv),
            "cat" => (0, format!("## DO NOT EDIT OR REMOVE THIS FILE!\n## If you remove this file, the iSCSI daemon will not start.\nInitiatorName={}\n", self.initiator)),
            "sh" => {
                // sh -c 'printf ... > /etc/iscsi/initiatorname.iscsi' sh NAME
                if let Some(name) = argv.get(4) {
                    self.initiator = name.clone();
                }
                (0, String::new())
            }
            "true" => (0, String::new()),
            "env" | "apt-get" => (0, "open-iscsi is already the newest version (2.1.11-1ubuntu1).\n0 upgraded, 0 newly installed, 0 to remove and 0 not upgraded.".into()),
            "gnome-disks" => (0, String::new()),
            other => (127, format!("{other}: command not found")),
        }
    }

    fn systemctl(&mut self, argv: &[String]) -> (i32, String) {
        let verb = argv.get(1).map(String::as_str).unwrap_or("");
        let units: Vec<&str> = argv.iter().skip(2).map(String::as_str).collect();
        let mut lines = Vec::new();
        let mut code = 0;
        for u in &units {
            let st = if u.starts_with("iscsid") { &mut self.iscsid } else { &mut self.open_iscsi };
            match verb {
                "is-active" => {
                    lines.push(if st.0 { "active" } else { "inactive" });
                    if !st.0 {
                        code = 3;
                    }
                }
                "is-enabled" => {
                    lines.push(if st.1 { "enabled" } else { "disabled" });
                    if !st.1 {
                        code = 1;
                    }
                }
                "start" | "restart" => st.0 = true,
                "stop" => st.0 = false,
                "enable" => st.1 = true,
                "disable" => st.1 = false,
                _ => {}
            }
        }
        (code, lines.join("\n"))
    }

    fn iscsiadm(&mut self, argv: &[String]) -> (i32, String) {
        let mode = Self::arg(argv, &["-m", "--mode"]).unwrap_or("");
        match mode {
            "session" => {
                if let Some(sid) = Self::arg(argv, &["-r", "--sid"]) {
                    let Some(n) = self.nodes.iter().find(|n| n.sid.map(|s| s.to_string()).as_deref() == Some(sid)) else {
                        return (21, format!("iscsiadm: Could not get session info for sid {sid}"));
                    };
                    return (0, format!("Rescanning session [sid: {sid}, target: {}, portal: {}]", n.target, Self::portal(n)));
                }
                self.session_p3()
            }
            "node" => self.node(argv),
            "discovery" | "discoverydb" => self.discovery(argv),
            _ => (7, "iscsiadm: Invalid mode".into()),
        }
    }

    fn session_p3(&mut self) -> (i32, String) {
        let mut logged: Vec<usize> = (0..self.nodes.len()).filter(|&i| self.nodes[i].sid.is_some()).collect();
        if logged.is_empty() {
            return (21, "iscsiadm: No active sessions.".into());
        }
        logged.sort_by_key(|&i| self.nodes[i].sid);
        let mut s = String::from("iSCSI Transport Class version 2.0-870\nversion 2.1.11\n");
        for (host, &i) in logged.iter().enumerate() {
            let n = &self.nodes[i];
            let sid = n.sid.unwrap();
            s += &format!("Target: {} (non-flash)\n", n.target);
            s += &format!("\tCurrent Portal: {},1\n\tPersistent Portal: {},1\n", Self::portal(n), Self::portal(n));
            s += "\t\t**********\n\t\tInterface:\n\t\t**********\n\t\tIface Name: default\n\t\tIface Transport: tcp\n";
            s += &format!("\t\tIface Initiatorname: {}\n\t\tIface IPaddress: 192.168.50.101\n", self.initiator);
            s += &format!("\t\tSID: {sid}\n\t\tiSCSI Connection State: LOGGED IN\n\t\tiSCSI Session State: LOGGED_IN\n\t\tInternal iscsid Session State: NO CHANGE\n");
            s += "\t\t************************\n\t\tAttached SCSI devices:\n\t\t************************\n";
            s += &format!("\t\tHost Number: {}\tState: running\n", host + 3);
            for (lun, disk) in n.luns.iter().zip(&n.disks) {
                s += &format!("\t\tscsi{} Channel 00 Id 0 Lun: {lun}\n", host + 3);
                s += &format!("\t\t\tAttached scsi disk {disk}\t\tState: running\n");
            }
        }
        (0, s)
    }

    fn node(&mut self, argv: &[String]) -> (i32, String) {
        let hits = self.find(argv);
        if Self::has(argv, "--loginall") {
            let all: Vec<usize> = (0..self.nodes.len()).filter(|&i| self.nodes[i].startup == "automatic").collect();
            let mut out = String::new();
            for i in all {
                let n = &self.nodes[i];
                out += &format!("Logging in to [iface: default, target: {}, portal: {}]\n", n.target, Self::portal(n));
                self.login(i);
            }
            return (0, out);
        }
        if Self::has(argv, "--logoutall") {
            let mut out = String::new();
            for n in self.nodes.iter_mut().filter(|n| n.sid.is_some()) {
                out += &format!("Logout of [sid: {}, target: {}, portal: {}] successful.\n", n.sid.unwrap(), n.target, Self::portal(n));
                n.sid = None;
                n.disks.clear();
            }
            return (if out.is_empty() { 21 } else { 0 }, if out.is_empty() { "iscsiadm: No matching sessions found".into() } else { out });
        }
        let has_selector = Self::has(argv, "-T") || Self::has(argv, "-p");
        if has_selector && hits.is_empty() {
            return (21, "iscsiadm: No records found".into());
        }
        if Self::has(argv, "--login") || Self::has(argv, "-l") {
            let mut out = String::new();
            for &i in &hits {
                let n = &self.nodes[i];
                if n.sid.is_some() {
                    return (15, format!("iscsiadm: default: 1 session requested, but 1 already present.\niscsiadm: Could not log into all portals"));
                }
                if n.target.ends_with(":scratch") {
                    return (24, format!("Logging in to [iface: default, target: {}, portal: {}]\niscsiadm: Could not login to [iface: default, target: {}, portal: {}].\niscsiadm: initiator reported error (24 - iSCSI login failed due to authorization failure)\niscsiadm: Could not log into all portals", n.target, Self::portal(n), n.target, Self::portal(n)));
                }
                out += &format!("Logging in to [iface: default, target: {}, portal: {}]\nLogin to [iface: default, target: {}, portal: {}] successful.\n", n.target, Self::portal(n), n.target, Self::portal(n));
                self.login(i);
            }
            return (0, out);
        }
        if Self::has(argv, "--logout") || Self::has(argv, "-u") {
            let mut out = String::new();
            for &i in &hits {
                let n = &mut self.nodes[i];
                n.disks.clear();
                if let Some(sid) = n.sid.take() {
                    out += &format!("Logging out of session [sid: {sid}, target: {}, portal: {}]\nLogout of [sid: {sid}, target: {}, portal: {}] successful.\n", n.target, Self::portal(n), n.target, Self::portal(n));
                }
            }
            if out.is_empty() {
                return (21, "iscsiadm: No matching sessions found".into());
            }
            return (0, out);
        }
        let op = Self::arg(argv, &["-o", "--op"]).unwrap_or("show");
        match op {
            "delete" => {
                let mut hits = hits;
                hits.sort_unstable_by(|a, b| b.cmp(a));
                for i in hits {
                    if self.nodes[i].sid.is_some() {
                        return (6, "iscsiadm: This command will remove the record, but a session is using it. Logout session then rerun command to remove record.".into());
                    }
                    self.nodes.remove(i);
                }
                (0, String::new())
            }
            "update" => {
                let key = Self::arg(argv, &["-n", "--name"]).unwrap_or("");
                let val = Self::arg(argv, &["-v", "--value"]).unwrap_or("").to_string();
                for &i in &hits {
                    let n = &mut self.nodes[i];
                    match key {
                        "node.startup" => n.startup = val.clone(),
                        "node.session.auth.authmethod" => n.auth = val.clone(),
                        "node.session.auth.username" => n.user = val.clone(),
                        "node.session.auth.username_in" => n.user_in = val.clone(),
                        _ => {}
                    }
                }
                (0, String::new())
            }
            _ => {
                if self.nodes.is_empty() {
                    return (21, "iscsiadm: No records found".into());
                }
                let mut s = String::new();
                let list: Vec<usize> = if has_selector { hits } else { (0..self.nodes.len()).collect() };
                for i in list {
                    let n = &self.nodes[i];
                    let e = |v: &str| if v.is_empty() { "<empty>".to_string() } else { v.to_string() };
                    s += "# BEGIN RECORD 2.1.11\n";
                    s += &format!("node.name = {}\nnode.tpgt = 1\nnode.startup = {}\nnode.leading_login = No\n", n.target, n.startup);
                    s += "iface.iscsi_ifacename = default\niface.transport_name = tcp\n";
                    s += &format!("node.session.auth.authmethod = {}\nnode.session.auth.username = {}\nnode.session.auth.password = ********\n", n.auth, e(&n.user));
                    s += &format!("node.session.auth.username_in = {}\nnode.session.auth.password_in = ********\n", e(&n.user_in));
                    s += &format!("node.conn[0].address = {}\nnode.conn[0].port = {}\nnode.conn[0].startup = manual\n", n.address, n.port);
                    s += "# END RECORD\n";
                }
                (0, s)
            }
        }
    }

    fn discovery(&mut self, argv: &[String]) -> (i32, String) {
        let p = Self::arg(argv, &["-p", "--portal"]).unwrap_or("");
        let (addr, port) = split_portal(p);
        let port = port.unwrap_or(3260);
        let Some((_, targets)) = self.servers.iter().find(|(a, _)| *a == addr && port == 3260).cloned() else {
            return (
                4,
                format!("iscsiadm: cannot make connection to {addr}: Connection refused\niscsiadm: connection login retries (reopen_max) 5 exceeded\niscsiadm: Could not perform SendTargets discovery: encountered connection failure"),
            );
        };
        let mut out = String::new();
        for (t, luns) in targets {
            self.add_node(t, &addr, port, luns);
            out += &format!("{}:{port},1 {t}\n", addr);
        }
        (0, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn run(d: &mut Demo, a: &[&str]) -> (i32, String) {
        d.run(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn find_connect_disconnect_forget() {
        let mut d = Demo::new();
        let (c, out) = run(&mut d, &["iscsiadm", "-m", "session", "-P", "3"]);
        assert_eq!(c, 0);
        assert_eq!(parse_sessions(&out).len(), 2);

        let (c, out) = run(&mut d, &["iscsiadm", "-m", "discovery", "-t", "sendtargets", "-p", "192.168.50.10:3260"]);
        assert_eq!(c, 0);
        let found = parse_portal_target_lines(&out);
        assert_eq!(found.len(), 3);
        let (_, out) = run(&mut d, &["iscsiadm", "-m", "node", "-o", "show"]);
        assert_eq!(parse_node_records(&out).len(), 4);

        let t = "iqn.2025-01.lab.storage:backup";
        let (c, _) = run(&mut d, &["iscsiadm", "-m", "node", "-T", t, "-p", "192.168.50.10:3260", "--login"]);
        assert_eq!(c, 0);
        let (_, out) = run(&mut d, &["iscsiadm", "-m", "session", "-P", "3"]);
        let s = parse_sessions(&out);
        assert_eq!(s.len(), 3);
        assert!(s.iter().any(|s| s.target == t && s.disks.len() == 1));

        // forgetting a connected target is refused; after logout it works
        let (c, _) = run(&mut d, &["iscsiadm", "-m", "node", "-T", t, "-p", "192.168.50.10:3260", "-o", "delete"]);
        assert_eq!(c, 6);
        let (c, _) = run(&mut d, &["iscsiadm", "-m", "node", "-T", t, "-p", "192.168.50.10:3260", "--logout"]);
        assert_eq!(c, 0);
        let (c, _) = run(&mut d, &["iscsiadm", "-m", "node", "-T", t, "-p", "192.168.50.10:3260", "-o", "delete"]);
        assert_eq!(c, 0);
        let (_, out) = run(&mut d, &["iscsiadm", "-m", "node", "-o", "show"]);
        assert_eq!(parse_node_records(&out).len(), 3);

        // the "scratch" target refuses logins (shows the CHAP failure path)
        let (c, _) = run(&mut d, &["iscsiadm", "-m", "node", "-T", "iqn.2025-01.lab.storage:scratch", "-p", "192.168.50.10:3260", "--login"]);
        assert_eq!(c, 24);
        // unknown server
        let (c, _) = run(&mut d, &["iscsiadm", "-m", "discovery", "-t", "sendtargets", "-p", "10.9.9.9:3260"]);
        assert_eq!(c, 4);
    }

    #[test]
    fn startup_and_chap_settings() {
        let mut d = Demo::new();
        let t = "iqn.2025-03.lab.nas:media";
        let p = "192.168.50.20:3260";
        run(&mut d, &["iscsiadm", "-m", "node", "-T", t, "-p", p, "--op", "update", "-n", "node.startup", "-v", "manual"]);
        run(&mut d, &["iscsiadm", "-m", "node", "-T", t, "-p", p, "--op", "update", "-n", "node.session.auth.authmethod", "-v", "CHAP"]);
        run(&mut d, &["iscsiadm", "-m", "node", "-T", t, "-p", p, "--op", "update", "-n", "node.session.auth.username", "-v", "me"]);
        let (_, out) = run(&mut d, &["iscsiadm", "-m", "node", "-o", "show"]);
        let n = parse_node_records(&out).into_iter().find(|n| n.target == t).unwrap();
        assert!(!n.automatic());
        assert!(n.uses_chap());
        assert_eq!(n.auth_user, "me");
        let (_, out) = run(&mut d, &["iscsiadm", "-m", "node", "--logoutall=all"]);
        assert!(out.contains("successful"));
        let (c, _) = run(&mut d, &["iscsiadm", "-m", "session", "-P", "3"]);
        assert_eq!(c, 21);
    }
}
