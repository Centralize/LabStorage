//! Data types and parsers for the text that `iscsiadm` (open-iscsi) prints.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    pub target: String,
    pub address: String,
    pub port: u16,
    pub tpgt: String,
    pub iface: String,
    pub startup: String,
    pub auth_method: String,
    pub auth_user: String,
    pub auth_user_in: String,
}

impl Node {
    pub fn portal(&self) -> String {
        fmt_portal(&self.address, self.port)
    }
    pub fn automatic(&self) -> bool {
        self.startup.eq_ignore_ascii_case("automatic") || self.startup.eq_ignore_ascii_case("onboot")
    }
    pub fn uses_chap(&self) -> bool {
        self.auth_method.eq_ignore_ascii_case("chap")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Disk {
    pub name: String,
    pub lun: String,
    pub state: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub sid: String,
    pub target: String,
    pub portal: String,
    pub iface_ip: String,
    pub conn_state: String,
    pub session_state: String,
    pub disks: Vec<Disk>,
}

impl Session {
    pub fn healthy(&self) -> bool {
        let s = self.session_state.to_ascii_uppercase();
        s.is_empty() || s.contains("LOGGED_IN") || s.contains("LOGGED IN")
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Discovered {
    pub portal: String,
    pub tpgt: String,
    pub target: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServiceInfo {
    pub name: String,
    pub active: String,
    pub enabled: String,
}

/// `addr:port`, with brackets for IPv6 addresses.
pub fn fmt_portal(addr: &str, port: u16) -> String {
    if addr.contains(':') && !addr.starts_with('[') {
        format!("[{addr}]:{port}")
    } else {
        format!("{addr}:{port}")
    }
}

/// Split `192.168.1.10:3260,1` into (`192.168.1.10:3260`, `1`).
pub fn split_tpgt(token: &str) -> (String, String) {
    match token.rsplit_once(',') {
        Some((p, t)) => (p.to_string(), t.to_string()),
        None => (token.to_string(), String::new()),
    }
}

/// Split a portal `addr:port` (or `[v6]:port`) into its parts.
pub fn split_portal(portal: &str) -> (String, Option<u16>) {
    let portal = portal.trim();
    if let Some(rest) = portal.strip_prefix('[') {
        if let Some((addr, tail)) = rest.split_once(']') {
            let port = tail.strip_prefix(':').and_then(|p| p.parse().ok());
            return (addr.to_string(), port);
        }
    }
    if portal.matches(':').count() == 1 {
        if let Some((a, p)) = portal.rsplit_once(':') {
            return (a.to_string(), p.parse().ok());
        }
    }
    (portal.to_string(), None)
}

fn clean_value(v: &str) -> String {
    let v = v.trim();
    if v == "<empty>" {
        String::new()
    } else {
        v.to_string()
    }
}

/// Parse `iscsiadm -m node -o show` (one or many `# BEGIN RECORD` blocks).
pub fn parse_node_records(text: &str) -> Vec<Node> {
    let mut out = Vec::new();
    let mut cur: Option<Node> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("# BEGIN RECORD") {
            if let Some(n) = cur.take() {
                if !n.target.is_empty() {
                    out.push(n);
                }
            }
            cur = Some(Node { port: 3260, ..Default::default() });
            continue;
        }
        if line.starts_with("# END RECORD") {
            if let Some(n) = cur.take() {
                if !n.target.is_empty() {
                    out.push(n);
                }
            }
            continue;
        }
        let Some((k, v)) = line.split_once(" = ") else { continue };
        let node = cur.get_or_insert_with(|| Node { port: 3260, ..Default::default() });
        let v = clean_value(v);
        match k.trim() {
            "node.name" => node.target = v,
            "node.tpgt" => node.tpgt = v,
            "node.startup" => node.startup = v,
            "iface.iscsi_ifacename" => node.iface = v,
            "node.session.auth.authmethod" => node.auth_method = v,
            "node.session.auth.username" => node.auth_user = v,
            "node.session.auth.username_in" => node.auth_user_in = v,
            "node.conn[0].address" => node.address = v,
            "node.conn[0].port" => node.port = v.parse().unwrap_or(3260),
            _ => {}
        }
    }
    if let Some(n) = cur.take() {
        if !n.target.is_empty() {
            out.push(n);
        }
    }
    out
}

/// Parse lines like `192.168.1.10:3260,1 iqn.2003-01.org.example:disk1`
/// (output of `iscsiadm -m discovery ...` and plain `iscsiadm -m node`).
pub fn parse_portal_target_lines(text: &str) -> Vec<Discovered> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(p), Some(t)) = (parts.next(), parts.next()) else { continue };
        if !(t.starts_with("iqn.") || t.starts_with("eui.") || t.starts_with("naa.")) {
            continue;
        }
        let (portal, tpgt) = split_tpgt(p);
        out.push(Discovered { portal, tpgt, target: t.to_string() });
    }
    out
}

/// Parse `iscsiadm -m session -P 3` (also accepts the short `-m session` form).
pub fn parse_sessions(text: &str) -> Vec<Session> {
    let mut out: Vec<Session> = Vec::new();
    let mut target = String::new();
    let mut pending_lun = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("Target:") {
            target = rest.split_whitespace().next().unwrap_or("").to_string();
        } else if let Some(rest) = line.strip_prefix("Current Portal:") {
            let (portal, _) = split_tpgt(rest.trim());
            out.push(Session { target: target.clone(), portal, ..Default::default() });
        } else if let Some(rest) = line.strip_prefix("Iface IPaddress:") {
            if let Some(s) = out.last_mut() {
                s.iface_ip = clean_value(rest);
            }
        } else if let Some(rest) = line.strip_prefix("SID:") {
            if let Some(s) = out.last_mut() {
                s.sid = rest.trim().to_string();
            }
        } else if let Some(rest) = line.strip_prefix("iSCSI Connection State:") {
            if let Some(s) = out.last_mut() {
                s.conn_state = rest.trim().to_string();
            }
        } else if let Some(rest) = line.strip_prefix("iSCSI Session State:") {
            if let Some(s) = out.last_mut() {
                s.session_state = rest.trim().to_string();
            }
        } else if line.starts_with("scsi") && line.contains("Lun:") {
            pending_lun = line.rsplit("Lun:").next().unwrap_or("").trim().to_string();
        } else if line.starts_with("Attached scsi disk") {
            let words: Vec<&str> = line.split_whitespace().collect();
            let name = words.get(3).copied().unwrap_or("").to_string();
            let state = words
                .iter()
                .position(|w| *w == "State:")
                .and_then(|i| words.get(i + 1))
                .copied()
                .unwrap_or("")
                .to_string();
            if let Some(s) = out.last_mut() {
                s.disks.push(Disk { name, lun: pending_lun.clone(), state });
            }
        } else if line.starts_with("tcp:") || line.starts_with("iser:") {
            // Short form: tcp: [1] 192.168.1.10:3260,1 iqn.... (non-flash)
            let words: Vec<&str> = line.split_whitespace().collect();
            if words.len() >= 4 {
                let sid = words[1].trim_matches(|c| c == '[' || c == ']').to_string();
                let (portal, _) = split_tpgt(words[2]);
                out.push(Session {
                    sid,
                    portal,
                    target: words[3].to_string(),
                    ..Default::default()
                });
            }
        }
    }
    out
}

/// Read `InitiatorName=...` from /etc/iscsi/initiatorname.iscsi.
pub fn parse_initiator(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| l.strip_prefix("InitiatorName="))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A simple sanity check for an iSCSI qualified name.
pub fn valid_iqn(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains(char::is_whitespace) {
        return false;
    }
    if let Some(rest) = s.strip_prefix("iqn.") {
        // iqn.YYYY-MM.reversed.domain[:anything]
        let b = rest.as_bytes();
        return b.len() > 8
            && b[..4].iter().all(u8::is_ascii_digit)
            && b[4] == b'-'
            && b[5..7].iter().all(u8::is_ascii_digit)
            && b[7] == b'.';
    }
    s.starts_with("eui.") || s.starts_with("naa.")
}

/// The friendly part of a target name: everything after the last ':'.
pub fn short_name(iqn: &str) -> &str {
    match iqn.rsplit_once(':') {
        Some((_, tail)) if !tail.is_empty() => tail,
        _ => iqn,
    }
}

pub fn human_bytes(b: u64) -> String {
    const U: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else if v >= 100.0 {
        format!("{v:.0} {}", U[i])
    } else {
        format!("{v:.1} {}", U[i])
    }
}

/// Read the connected sessions straight from the kernel (/sys/class/iscsi_*),
/// which normal users may read, unlike some files iscsiadm needs.
pub fn read_sysfs_sessions(sys: &std::path::Path) -> Vec<Session> {
    use std::fs;
    let read = |p: std::path::PathBuf| fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(sys.join("class/iscsi_session")) else { return out };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(sid) = name.strip_prefix("session") else { continue };
        let dir = e.path();
        let mut s = Session {
            sid: sid.to_string(),
            target: read(dir.join("targetname")),
            session_state: read(dir.join("state")),
            ..Default::default()
        };
        let Ok(dev) = fs::canonicalize(dir.join("device")) else {
            out.push(s);
            continue;
        };
        // Local address from the host the session belongs to.
        if let Some(host) = dev.parent().and_then(|p| p.file_name()) {
            s.iface_ip = read(sys.join("class/iscsi_host").join(host).join("ipaddress"));
        }
        let mut targets = Vec::new();
        for c in fs::read_dir(&dev).into_iter().flatten().flatten() {
            let cname = c.file_name().to_string_lossy().into_owned();
            if cname.starts_with("connection") && s.portal.is_empty() {
                let conn = sys.join("class/iscsi_connection").join(&cname);
                let mut addr = read(conn.join("persistent_address"));
                let mut port = read(conn.join("persistent_port"));
                if addr.is_empty() {
                    addr = read(conn.join("address"));
                    port = read(conn.join("port"));
                }
                if !addr.is_empty() {
                    s.portal = fmt_portal(&addr, port.parse().unwrap_or(3260));
                }
                s.conn_state = read(conn.join("state"));
            } else if cname.starts_with("target") {
                targets.push(c.path());
            }
        }
        // Disks: <session>/target*/H:C:T:L/block/<name>
        let mut disks = Vec::new();
        for t in targets {
            for lun in fs::read_dir(&t).into_iter().flatten().flatten() {
                let hctl = lun.file_name().to_string_lossy().into_owned();
                if hctl.split(':').count() != 4 {
                    continue;
                }
                for b in fs::read_dir(lun.path().join("block")).into_iter().flatten().flatten() {
                    disks.push(Disk {
                        name: b.file_name().to_string_lossy().into_owned(),
                        lun: hctl.rsplit(':').next().unwrap_or("").to_string(),
                        state: read(lun.path().join("state")),
                    });
                }
            }
        }
        disks.sort_by_key(|d| d.lun.parse::<u64>().unwrap_or(u64::MAX));
        s.disks = disks;
        out.push(s);
    }
    out.sort_by_key(|s| s.sid.parse::<u64>().unwrap_or(u64::MAX));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const P3: &str = "iSCSI Transport Class version 2.0-870
version 2.1.9
Target: iqn.2025-01.lab.storage:vm-store (non-flash)
\tCurrent Portal: 192.168.50.10:3260,1
\tPersistent Portal: 192.168.50.10:3260,1
\t\t**********
\t\tInterface:
\t\t**********
\t\tIface Name: default
\t\tIface IPaddress: 192.168.50.101
\t\tSID: 1
\t\tiSCSI Connection State: LOGGED IN
\t\tiSCSI Session State: LOGGED_IN
\t\tInternal iscsid Session State: NO CHANGE
\t\t************************
\t\tAttached SCSI devices:
\t\t************************
\t\tHost Number: 3\tState: running
\t\tscsi3 Channel 00 Id 0 Lun: 0
\t\t\tAttached scsi disk sdb\t\tState: running
\t\tscsi3 Channel 00 Id 0 Lun: 1
\t\t\tAttached scsi disk sdc\t\tState: running
Target: iqn.2025-03.lab.nas:media (non-flash)
\tCurrent Portal: [fd00::20]:3260,1
\tPersistent Portal: [fd00::20]:3260,1
\t\tSID: 2
\t\tiSCSI Connection State: LOGGED IN
\t\tiSCSI Session State: LOGGED_IN
";

    #[test]
    fn sessions() {
        let s = parse_sessions(P3);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].sid, "1");
        assert_eq!(s[0].portal, "192.168.50.10:3260");
        assert_eq!(s[0].disks.len(), 2);
        assert_eq!(s[0].disks[1].name, "sdc");
        assert_eq!(s[0].disks[1].lun, "1");
        assert_eq!(s[0].disks[1].state, "running");
        assert_eq!(s[1].target, "iqn.2025-03.lab.nas:media");
        assert_eq!(s[1].portal, "[fd00::20]:3260");
    }

    #[test]
    fn sysfs_sessions() {
        use std::fs;
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("iscsi-sysfs-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let dev = root.join("devices/platform/host8/session1");
        for l in ["8:0:0:1", "8:0:0:2"] {
            fs::create_dir_all(dev.join(format!("target8:0:0/{l}/block/sd{}", if l.ends_with('1') { "a" } else { "b" }))).unwrap();
            fs::write(dev.join(format!("target8:0:0/{l}/state")), "running\n").unwrap();
        }
        fs::create_dir_all(dev.join("connection1:0")).unwrap();
        let sess = root.join("class/iscsi_session/session1");
        fs::create_dir_all(&sess).unwrap();
        fs::write(sess.join("targetname"), "iqn.2000-01.com.synology:AeroSAN.default-target.bef2ee01cdb\n").unwrap();
        fs::write(sess.join("state"), "LOGGED_IN\n").unwrap();
        symlink(&dev, sess.join("device")).unwrap();
        let conn = root.join("class/iscsi_connection/connection1:0");
        fs::create_dir_all(&conn).unwrap();
        fs::write(conn.join("persistent_address"), "192.168.64.5\n").unwrap();
        fs::write(conn.join("persistent_port"), "3260\n").unwrap();
        let host = root.join("class/iscsi_host/host8");
        fs::create_dir_all(&host).unwrap();
        fs::write(host.join("ipaddress"), "192.168.64.10\n").unwrap();

        let s = read_sysfs_sessions(&root);
        let _ = fs::remove_dir_all(&root);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].sid, "1");
        assert_eq!(s[0].portal, "192.168.64.5:3260");
        assert_eq!(s[0].iface_ip, "192.168.64.10");
        assert!(s[0].healthy());
        let names: Vec<_> = s[0].disks.iter().map(|d| (d.name.as_str(), d.lun.as_str())).collect();
        assert_eq!(names, [("sda", "1"), ("sdb", "2")]);
    }

    #[test]
    fn short_sessions() {
        let s = parse_sessions("tcp: [4] 10.0.0.5:3260,1 iqn.2001-04.com.example:a (non-flash)\n");
        assert_eq!(s[0].sid, "4");
        assert_eq!(s[0].target, "iqn.2001-04.com.example:a");
    }

    #[test]
    fn records() {
        let t = "# BEGIN RECORD 2.1.9
node.name = iqn.2025-01.lab.storage:vm-store
node.tpgt = 1
node.startup = automatic
iface.iscsi_ifacename = default
node.session.auth.authmethod = CHAP
node.session.auth.username = labuser
node.session.auth.password = ********
node.session.auth.username_in = <empty>
node.conn[0].address = 192.168.50.10
node.conn[0].port = 3260
# END RECORD
# BEGIN RECORD 2.1.9
node.name = iqn.2025-01.lab.storage:backup
node.startup = manual
node.session.auth.authmethod = None
node.conn[0].address = fd00::20
node.conn[0].port = 3261
# END RECORD
";
        let n = parse_node_records(t);
        assert_eq!(n.len(), 2);
        assert!(n[0].automatic());
        assert!(n[0].uses_chap());
        assert_eq!(n[0].auth_user, "labuser");
        assert_eq!(n[0].auth_user_in, "");
        assert_eq!(n[0].portal(), "192.168.50.10:3260");
        assert_eq!(n[1].portal(), "[fd00::20]:3261");
    }

    #[test]
    fn discovery() {
        let d = parse_portal_target_lines(
            "192.168.50.10:3260,1 iqn.2025-01.lab.storage:vm-store\n[fd00::20]:3260,1 iqn.2025-03.lab.nas:media\niscsiadm: junk\n",
        );
        assert_eq!(d.len(), 2);
        assert_eq!(d[1].portal, "[fd00::20]:3260");
        assert_eq!(split_portal(&d[1].portal), ("fd00::20".into(), Some(3260)));
        assert_eq!(split_portal("nas.local:3261"), ("nas.local".into(), Some(3261)));
    }

    #[test]
    fn initiator_and_iqn() {
        assert_eq!(
            parse_initiator("## DO NOT EDIT\nInitiatorName=iqn.2004-10.com.ubuntu:01:abc\n").as_deref(),
            Some("iqn.2004-10.com.ubuntu:01:abc")
        );
        assert!(valid_iqn("iqn.2004-10.com.ubuntu:01:abc"));
        assert!(!valid_iqn("iqn.20x4-10.com"));
        assert!(!valid_iqn("hello"));
        assert_eq!(short_name("iqn.2025-01.lab.storage:vm-store"), "vm-store");
        assert_eq!(human_bytes(107374182400), "100 GiB");
    }
}
