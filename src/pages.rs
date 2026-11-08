//! The window's panels and pages.

use crate::app::*;
use crate::exec::{Job, JobKind, Root, Step};
use crate::model::*;
use crate::ui::*;
use egui::{Align, Color32, CornerRadius, Layout, Margin, RichText, Stroke, Vec2};
use std::time::Duration;

impl IscsiApp {
    pub fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            logo(ui);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("iSCSI Manager").size(19.0).strong().color(TEXT));
                ui.label(RichText::new("Network storage connections for Ubuntu Desktop").size(12.0).color(MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(accent_button("Refresh")).on_hover_text("Read the current state again").clicked() {
                    self.refresh();
                }
                ui.add_space(4.0);
                if self.runner.is_unlocked() {
                    chip(ui, "Administrator: unlocked", GOOD);
                } else if ui
                    .add(egui::Button::new(RichText::new("Unlock administrator").color(TEXT)).fill(CARD_HI).min_size(Vec2::new(0.0, 30.0)))
                    .on_hover_text("Ask for your password once, so connecting and changing settings works without asking again")
                    .clicked()
                {
                    self.send(unlock_job());
                }
                if self.pending > 0 {
                    ui.add_space(6.0);
                    ui.add(egui::Spinner::new().size(18.0).color(ACCENT));
                    ui.label(RichText::new("Working…").color(MUTED));
                }
            });
        });
    }

    pub fn demo_banner(&mut self, ui: &mut egui::Ui) {
        if !self.demo {
            return;
        }
        let mut install = false;
        egui::Frame::new()
            .fill(Color32::from_rgb(0x3a, 0x31, 0x17))
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x6b, 0x57, 0x1d)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    dot(ui, WARN, 5.0);
                    let msg = if self.force_demo {
                        "Sample mode: a pretend storage network (started with --demo). Nothing here changes your computer."
                    } else {
                        "Sample mode: open-iscsi isn't installed, so this shows a pretend storage network. Nothing here changes your computer."
                    };
                    ui.label(RichText::new(msg).color(Color32::from_rgb(0xf2, 0xdc, 0x9b)));
                    if !self.force_demo {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            install = ui.add(egui::Button::new(RichText::new("Install open-iscsi").color(Color32::BLACK)).fill(WARN)).clicked();
                        });
                    }
                });
            });
        if install {
            self.install();
        }
        ui.add_space(10.0);
    }

    pub fn nav(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let errors = self.log.iter().filter(|l| !l.ok).count();
        let items = [
            (Page::Connections, "Connected storage", "Sessions and their disks", self.sessions.len().to_string()),
            (Page::Targets, "Saved targets", "Connect, disconnect, startup", self.nodes.len().to_string()),
            (Page::Discover, "Find targets", "Ask a storage server", String::new()),
            (Page::Computer, "This computer", "Name, services, software", String::new()),
            (Page::Activity, "Activity", "Commands and their output", if errors > 0 { format!("{errors}!") } else { self.log.len().to_string() }),
        ];
        for (page, title, sub, count) in items {
            let selected = self.page == page;
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 50.0), egui::Sense::click());
            let fill = if selected {
                CARD_HI
            } else if resp.hovered() {
                Color32::from_rgb(0x1f, 0x25, 0x30)
            } else {
                Color32::TRANSPARENT
            };
            let p = ui.painter();
            p.rect_filled(rect, CornerRadius::same(8), fill);
            if selected {
                let bar = egui::Rect::from_min_size(rect.min + Vec2::new(0.0, 10.0), Vec2::new(3.0, rect.height() - 20.0));
                p.rect_filled(bar, CornerRadius::same(2), ACCENT);
            }
            p.text(
                rect.min + Vec2::new(14.0, 9.0),
                egui::Align2::LEFT_TOP,
                title,
                egui::FontId::proportional(14.5),
                if selected { TEXT } else { Color32::from_rgb(0xc8, 0xce, 0xd8) },
            );
            p.text(rect.min + Vec2::new(14.0, 29.0), egui::Align2::LEFT_TOP, sub, egui::FontId::proportional(11.5), MUTED);
            if !count.is_empty() {
                let warn = count.ends_with('!');
                let txt = count.trim_end_matches('!').to_string();
                let galley = p.layout_no_wrap(txt, egui::FontId::proportional(12.0), if warn { Color32::BLACK } else { TEXT });
                let bw = galley.size().x + 14.0;
                let br = egui::Rect::from_center_size(egui::pos2(rect.max.x - 10.0 - bw / 2.0, rect.center().y), Vec2::new(bw, 20.0));
                p.rect_filled(br, CornerRadius::same(10), if warn { WARN } else { LINE });
                p.galley(br.center() - galley.size() / 2.0, galley, TEXT);
            }
            if resp.clicked() {
                self.page = page;
            }
            ui.add_space(4.0);
        }
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let n: usize = self.sessions.iter().map(|s| s.disks.len()).sum();
                ui.label(RichText::new(format!("{n} disk{} attached", if n == 1 { "" } else { "s" })).size(12.0).color(MUTED));
            });
            let (txt, col) = match self.services.iter().find(|s| s.name.starts_with("iscsid")) {
                Some(s) if s.active == "active" => ("iSCSI service running", GOOD),
                Some(_) if self.loaded => ("iSCSI service stopped", BAD),
                _ => ("Checking service…", MUTED),
            };
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                dot(ui, col, 4.0);
                ui.label(RichText::new(txt).size(12.0).color(MUTED));
            });
        });
    }

    pub fn page_connections(&mut self, ui: &mut egui::Ui) {
        let sessions = self.sessions.clone();
        let disks: usize = sessions.iter().map(|s| s.disks.len()).sum();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                page_header(
                    ui,
                    "Connected storage",
                    &format!(
                        "{} active connection{} · {} disk{} attached",
                        sessions.len(),
                        if sessions.len() == 1 { "" } else { "s" },
                        disks,
                        if disks == 1 { "" } else { "s" }
                    ),
                );
            });
            if !sessions.is_empty() {
                ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                    if ui.add(danger_button("Disconnect all")).clicked() {
                        self.confirm = Some(Confirm {
                            title: "Disconnect all storage?".into(),
                            body: "Every connected iSCSI disk will be removed from this computer. Close files and unmount the disks first so nothing is lost.".into(),
                            button: "Disconnect all".into(),
                            job: Job {
                                kind: JobKind::Action,
                                label: "Disconnect all".into(),
                                steps: vec![Step::new(&["iscsiadm", "-m", "node", "--logoutall=all"], Root::Yes)],
                                keep_going: false,
                                quiet: false,
                            },
                        });
                    }
                });
            }
        });

        if sessions.is_empty() {
            card(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(18.0);
                    ui.label(RichText::new(if self.loaded { "Nothing is connected right now" } else { "Reading connections…" }).size(17.0).color(TEXT));
                    ui.label(RichText::new("Connect a saved target, or ask a storage server which targets it offers.").color(MUTED));
                    ui.add_space(10.0);
                    if ui.add(accent_button("Saved targets")).clicked() {
                        self.page = Page::Targets;
                    }
                    if ui.button("Find targets").clicked() {
                        self.page = Page::Discover;
                    }
                    ui.add_space(18.0);
                });
            });
            return;
        }

        for s in &sessions {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    dot(ui, if s.healthy() { GOOD } else { WARN }, 5.0);
                    ui.label(RichText::new(short_name(&s.target)).size(16.5).strong().color(TEXT));
                    chip(ui, if s.healthy() { "Connected" } else { "Reconnecting" }, if s.healthy() { GOOD } else { WARN });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(danger_button("Disconnect")).clicked() {
                            self.confirm = Some(Confirm {
                                title: format!("Disconnect {}?", short_name(&s.target)),
                                body: format!(
                                    "Its disk{} will disappear from this computer. Close files and unmount first so nothing is lost.",
                                    if s.disks.len() == 1 { "" } else { "s" }
                                ),
                                button: "Disconnect".into(),
                                job: Job {
                                    kind: JobKind::Action,
                                    label: format!("Disconnect {}", short_name(&s.target)),
                                    steps: vec![Self::node_step(&s.target, &s.portal, &["--logout"])],
                                    keep_going: false,
                                    quiet: false,
                                },
                            });
                        }
                        if ui.button("Rescan disks").on_hover_text("Look for new or resized disks on this target").clicked() && !s.sid.is_empty() {
                            let st = Step::new(&["iscsiadm", "-m", "session", "-r", &s.sid, "--rescan"], Root::Yes);
                            self.action(format!("Rescan {}", short_name(&s.target)), vec![st]);
                        }
                    });
                });
                ui.label(RichText::new(&s.target).monospace().size(12.0).color(MUTED));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    fact(ui, "Server", &s.portal);
                    fact(ui, "Session", &format!("#{}", s.sid));
                    if !s.iface_ip.is_empty() {
                        fact(ui, "Local address", &s.iface_ip);
                    }
                    if !s.conn_state.is_empty() {
                        fact(ui, "Link", &s.conn_state.to_lowercase());
                    }
                });
                ui.add_space(6.0);
                if s.disks.is_empty() {
                    ui.label(RichText::new("No disks reported yet. Try Rescan disks.").color(MUTED));
                }
                for d in &s.disks {
                    let info = self.disks.get(&d.name).cloned().unwrap_or_default();
                    let mut open = false;
                    egui::Frame::new().fill(BG).corner_radius(CornerRadius::same(6)).inner_margin(Margin::symmetric(10, 7)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            disk_icon(ui);
                            ui.label(RichText::new(format!("/dev/{}", d.name)).monospace().strong().color(TEXT));
                            ui.label(RichText::new(format!("LUN {}", d.lun)).color(MUTED));
                            if info.size > 0 {
                                ui.label(RichText::new(human_bytes(info.size)).color(TEXT));
                            }
                            if !info.model.is_empty() {
                                ui.label(RichText::new(&info.model).color(MUTED));
                            }
                            if info.mounts.is_empty() {
                                chip(ui, "Not mounted", MUTED);
                            } else {
                                chip(ui, &format!("Mounted at {}", info.mounts.join(", ")), ACCENT);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                open = ui.small_button("Open in Disks").on_hover_text("Format, partition or mount it with the Disks app").clicked();
                            });
                        });
                    });
                    if open {
                        self.open_disks_app(&d.name);
                    }
                    ui.add_space(4.0);
                }
            });
        }
    }

    pub fn page_targets(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Saved targets", "Storage this computer knows about. Targets set to connect at startup come back after a reboot.");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter by name or server").desired_width(260.0));
            if ui.add(accent_button("Find more targets")).clicked() {
                self.page = Page::Discover;
            }
            if ui.button("Connect all startup targets").on_hover_text("Connect every target set to connect at startup").clicked() {
                self.action(
                    "Connect startup targets",
                    vec![Step::new(&["iscsiadm", "-m", "node", "--loginall=automatic"], Root::Yes).ok_codes(&[0, 15, 21])],
                );
            }
        });
        ui.add_space(8.0);

        if !self.demo && !self.nodes_elevated {
            let mut unlock = false;
            note(
                ui,
                WARN,
                "Saved targets are kept in a folder only administrators can read. Unlock administrator access to make sure every saved target is listed.",
                |ui| unlock = ui.button("Unlock").clicked(),
            );
            if unlock {
                self.send(unlock_job());
            }
        }
        if let Some(e) = self.nodes_error.clone() {
            note(ui, BAD, &format!("Couldn't read the saved targets: {e}"), |_| {});
        }

        let f = self.filter.to_lowercase();
        let nodes: Vec<Node> = self
            .nodes
            .iter()
            .filter(|n| f.is_empty() || n.target.to_lowercase().contains(&f) || n.portal().to_lowercase().contains(&f))
            .cloned()
            .collect();
        if nodes.is_empty() {
            card(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(14.0);
                    ui.label(RichText::new(if self.nodes.is_empty() { "No saved targets yet" } else { "No targets match the filter" }).size(16.0).color(TEXT));
                    ui.label(RichText::new("Use Find targets to ask a storage server what it offers.").color(MUTED));
                    ui.add_space(14.0);
                });
            });
            return;
        }
        for n in &nodes {
            let portal = n.portal();
            let connected = self.session_for(&n.target, &portal).is_some();
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    dot(ui, if connected { GOOD } else { MUTED }, 5.0);
                    ui.label(RichText::new(short_name(&n.target)).size(16.0).strong().color(TEXT));
                    chip(ui, if connected { "Connected" } else { "Not connected" }, if connected { GOOD } else { MUTED });
                    if n.uses_chap() {
                        chip(ui, "Password login (CHAP)", ACCENT);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Remove").on_hover_text("Forget this target on this computer").clicked() {
                            let mut steps = Vec::new();
                            if connected {
                                steps.push(Self::node_step(&n.target, &portal, &["--logout"]));
                            }
                            steps.push(Self::node_step(&n.target, &portal, &["-o", "delete"]));
                            self.confirm = Some(Confirm {
                                title: format!("Forget {}?", short_name(&n.target)),
                                body: format!(
                                    "The saved settings for this target are removed from this computer{}. The data on the storage server is not touched; you can find it again later.",
                                    if connected { " and it is disconnected first" } else { "" }
                                ),
                                button: "Forget".into(),
                                job: Job { kind: JobKind::Action, label: format!("Forget {}", short_name(&n.target)), steps, keep_going: false, quiet: false },
                            });
                        }
                        if ui.button("Login settings…").clicked() {
                            self.chap = Some(ChapEdit {
                                target: n.target.clone(),
                                portal: portal.clone(),
                                chap: n.uses_chap(),
                                user: n.auth_user.clone(),
                                pass: String::new(),
                                mutual: !n.auth_user_in.is_empty(),
                                user_in: n.auth_user_in.clone(),
                                pass_in: String::new(),
                                connect_after: false,
                            });
                        }
                        if connected {
                            if ui.add(danger_button("Disconnect")).clicked() {
                                self.disconnect(&n.target, &portal);
                            }
                        } else if ui.add(accent_button("Connect")).clicked() {
                            self.connect(&n.target, &portal);
                        }
                    });
                });
                ui.label(RichText::new(&n.target).monospace().size(12.0).color(MUTED));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    fact(ui, "Server", &portal);
                    if !n.iface.is_empty() {
                        fact(ui, "Network interface", &n.iface);
                    }
                    ui.add_space(12.0);
                    let mut auto = n.automatic();
                    if ui.checkbox(&mut auto, "Connect at startup").on_hover_text("Reconnect this target automatically when the computer starts").changed() {
                        let v = if auto { "automatic" } else { "manual" };
                        if let Some(m) = self.nodes.iter_mut().find(|m| m.target == n.target && m.portal() == portal) {
                            m.startup = v.into();
                        }
                        self.action(
                            format!("{} {} at startup", if auto { "Connect" } else { "Don't connect" }, short_name(&n.target)),
                            vec![Self::update_step(&n.target, &portal, "node.startup", v)],
                        );
                    }
                });
            });
        }
    }

    pub fn page_discover(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "Find targets", "Ask a storage server (a NAS, SAN or Linux target) which disks it offers to this computer.");
        let mut go = false;
        card(ui, |ui| {
            egui::Grid::new("disc").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                ui.label(RichText::new("Server address").color(TEXT));
                let r = ui.add(egui::TextEdit::singleline(&mut self.discover_addr).hint_text("192.168.1.50 or nas.local").desired_width(300.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    go = true;
                }
                ui.end_row();
                ui.label(RichText::new("Port").color(TEXT));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.discover_port).desired_width(70.0));
                    ui.label(RichText::new("3260 is the usual iSCSI port").color(MUTED).size(12.0));
                });
                ui.end_row();
            });
            ui.add_space(8.0);
            let can = !self.discover_addr.trim().is_empty() && self.discover_port.trim().parse::<u16>().is_ok();
            go &= can;
            ui.horizontal(|ui| {
                if ui.add_enabled(can, accent_button("Find targets")).clicked() {
                    go = true;
                }
                ui.label(RichText::new("Found targets are saved on this computer, ready to connect.").color(MUTED).size(12.0));
            });
            let mut servers: Vec<String> = self.nodes.iter().map(|n| n.address.clone()).collect();
            servers.sort();
            servers.dedup();
            if !servers.is_empty() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Known servers:").color(MUTED).size(12.0));
                    for s in servers {
                        if ui.small_button(&s).clicked() {
                            self.discover_addr = s;
                        }
                    }
                });
            }
        });
        if go {
            self.discover();
        }

        if let Some(e) = self.discover_error.clone() {
            note(ui, BAD, &format!("No targets found on {}: {e}", self.discover_portal), |_| {});
        }
        if !self.discovered.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Offered by {}", self.discover_portal)).strong().size(15.0).color(TEXT));
            ui.add_space(4.0);
            for d in self.discovered.clone() {
                let connected = self.session_for(&d.target, &d.portal).is_some();
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        dot(ui, if connected { GOOD } else { ACCENT }, 5.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(short_name(&d.target)).strong().size(15.0).color(TEXT));
                            ui.label(RichText::new(format!("{}  ·  {}", d.target, d.portal)).monospace().size(12.0).color(MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if connected {
                                chip(ui, "Connected", GOOD);
                            } else {
                                if ui.add(accent_button("Connect")).clicked() {
                                    self.connect(&d.target, &d.portal);
                                }
                                if ui.button("Set password, then connect…").clicked() {
                                    self.chap = Some(ChapEdit {
                                        target: d.target.clone(),
                                        portal: d.portal.clone(),
                                        chap: true,
                                        user: String::new(),
                                        pass: String::new(),
                                        mutual: false,
                                        user_in: String::new(),
                                        pass_in: String::new(),
                                        connect_after: true,
                                    });
                                }
                            }
                        });
                    });
                });
            }
        }
    }

    pub fn page_computer(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "This computer", "How this computer introduces itself to storage servers, and the background services iSCSI needs.");
        card(ui, |ui| {
            section_title(ui, "iSCSI name (initiator name)");
            ui.label(
                RichText::new("Storage servers use this name to decide which computers may connect. Give it to your storage administrator or add it to the server's access list.")
                    .color(MUTED),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                match &self.initiator {
                    Some(n) => {
                        ui.label(RichText::new(n).monospace().size(15.0).color(TEXT));
                    }
                    None => {
                        ui.label(RichText::new(if self.runner.is_unlocked() { "Not set yet" } else { "Unlock administrator access to see it" }).color(MUTED));
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Change…").clicked() {
                        self.initiator_edit = Some(self.initiator.clone().unwrap_or_else(new_initiator_name));
                    }
                    if let Some(n) = self.initiator.clone() {
                        if ui.button("Copy").clicked() {
                            ui.ctx().copy_text(n);
                            self.toast(true, "iSCSI name copied.".into());
                        }
                    }
                });
            });
        });

        card(ui, |ui| {
            section_title(ui, "Background services");
            ui.label(RichText::new("iscsid talks to storage servers; open-iscsi reconnects targets set to connect at startup.").color(MUTED));
            ui.add_space(6.0);
            for s in self.services.clone() {
                let running = s.active == "active";
                let enabled = s.enabled == "enabled";
                ui.horizontal(|ui| {
                    dot(ui, if running { GOOD } else { BAD }, 5.0);
                    ui.label(RichText::new(s.name.trim_end_matches(".service")).strong().color(TEXT));
                    chip(ui, if running { "Running" } else { &s.active }, if running { GOOD } else { BAD });
                    chip(ui, if enabled { "Starts at boot" } else { "Doesn't start at boot" }, if enabled { ACCENT } else { MUTED });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let mut en = enabled;
                        if ui.checkbox(&mut en, "Start at boot").changed() {
                            self.action(
                                format!("{} {}", if en { "Enable" } else { "Disable" }, s.name),
                                vec![Step::owned(vec!["systemctl".into(), if en { "enable" } else { "disable" }.into(), s.name.clone()], Root::Yes)],
                            );
                        }
                        if ui.button("Restart").clicked() {
                            self.action(format!("Restart {}", s.name), vec![Step::owned(vec!["systemctl".into(), "restart".into(), s.name.clone()], Root::Yes)]);
                        }
                        if running {
                            if ui.button("Stop").clicked() {
                                self.action(format!("Stop {}", s.name), vec![Step::owned(vec!["systemctl".into(), "stop".into(), s.name.clone()], Root::Yes)]);
                            }
                        } else if ui.add(accent_button("Start")).clicked() {
                            self.action(format!("Start {}", s.name), vec![Step::owned(vec!["systemctl".into(), "start".into(), s.name.clone()], Root::Yes)]);
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });

        card(ui, |ui| {
            section_title(ui, "Administrator access");
            let mut unlock = false;
            ui.horizontal(|ui| {
                if self.runner.is_unlocked() {
                    dot(ui, GOOD, 5.0);
                    ui.label(RichText::new("Unlocked for this session. Commands run without asking for your password again until you close the app.").color(TEXT));
                } else {
                    dot(ui, MUTED, 5.0);
                    ui.label(RichText::new("Locked. You'll be asked for your password once, the first time a change needs it.").color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| unlock = ui.button("Unlock now").clicked());
                }
            });
            if unlock {
                self.send(unlock_job());
            }
        });

        card(ui, |ui| {
            section_title(ui, "Software");
            let mut install = false;
            ui.horizontal(|ui| match &self.iscsiadm {
                Some(p) => {
                    dot(ui, GOOD, 5.0);
                    ui.label(RichText::new(format!("open-iscsi is installed ({})", p.display())).color(TEXT));
                }
                None => {
                    dot(ui, WARN, 5.0);
                    ui.label(RichText::new("open-iscsi isn't installed. Ubuntu Desktop doesn't include it by default.").color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        install = ui.add(accent_button("Install open-iscsi")).clicked();
                    });
                }
            });
            if install {
                self.install();
            }
        });
    }

    pub fn page_activity(&mut self, ui: &mut egui::Ui) {
        let mut clear = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| page_header(ui, "Activity", "Every command this app ran, with what it printed. Passwords are hidden."));
            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| clear = ui.button("Clear").clicked());
        });
        if clear {
            self.log.clear();
        }
        if self.log.is_empty() {
            card(ui, |ui| {
                ui.label(RichText::new("Nothing has been done yet.").color(MUTED));
            });
        }
        let last = self.log.len();
        for (i, e) in self.log.iter().enumerate().rev() {
            card(ui, |ui| {
                let head = RichText::new(format!("{}   ·   {}   ·   took {:.1} s", e.label, ago(e.when), e.took.as_secs_f32())).color(TEXT).strong();
                ui.horizontal(|ui| {
                    dot(ui, if e.ok { GOOD } else { BAD }, 5.0);
                    ui.vertical(|ui| {
                        egui::CollapsingHeader::new(head).id_salt(("log", i)).default_open(!e.ok && i + 1 == last).show(ui, |ui| {
                            for (j, s) in e.steps.iter().enumerate() {
                                ui.label(RichText::new(format!("{} {}", if s.elevated { "#" } else { "$" }, s.display)).monospace().color(ACCENT));
                                ui.label(RichText::new(format!("exit code {}", s.code)).size(11.5).color(if s.ok { MUTED } else { BAD }));
                                if !s.output.trim().is_empty() {
                                    egui::Frame::new().fill(BG).corner_radius(CornerRadius::same(6)).inner_margin(Margin::same(8)).show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        egui::ScrollArea::vertical().max_height(180.0).id_salt(("out", i, j)).show(ui, |ui| {
                                            ui.label(RichText::new(s.output.trim_end()).monospace().size(12.0).color(Color32::from_rgb(0xc9, 0xd1, 0xdc)));
                                        });
                                    });
                                }
                                ui.add_space(4.0);
                            }
                        });
                    });
                });
            });
        }
    }

    pub fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let fresh = self.toast.as_ref().filter(|(_, _, w)| w.elapsed() < Duration::from_secs(10)).cloned();
            match fresh {
                Some((ok, msg, _)) => {
                    dot(ui, if ok { GOOD } else { BAD }, 4.0);
                    let r = ui.label(RichText::new(msg).color(if ok { TEXT } else { Color32::from_rgb(0xf3, 0xa6, 0xa0) }));
                    if !ok {
                        r.on_hover_text("See Activity for the full output");
                    }
                }
                None => {
                    ui.label(RichText::new(if self.loaded { "Ready" } else { "Reading the current state…" }).color(MUTED));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(n) = &self.initiator {
                    ui.label(RichText::new(n).monospace().size(12.0).color(MUTED));
                    ui.label(RichText::new("This computer:").size(12.0).color(MUTED));
                }
            });
        });
    }

    pub fn modals(&mut self, ctx: &egui::Context) {
        // Confirmation
        if let Some(c) = &self.confirm {
            let (title, body, button) = (c.title.clone(), c.body.clone(), c.button.clone());
            let mut go = false;
            let mut close = false;
            let resp = egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new(title).size(17.0).strong().color(TEXT));
                ui.add_space(6.0);
                ui.label(RichText::new(body).color(MUTED));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    go = ui.add(danger_button(&button)).clicked();
                    close = ui.button("Cancel").clicked();
                });
            });
            if go {
                let job = self.confirm.take().unwrap().job;
                self.send(job);
            } else if close || resp.should_close() {
                self.confirm = None;
            }
        }

        // CHAP / login settings
        if self.chap.is_some() {
            let mut save = false;
            let mut close = false;
            let resp = egui::Modal::new(egui::Id::unique("chap")).show(ctx, |ui| {
                let c = self.chap.as_mut().unwrap();
                ui.set_width(470.0);
                ui.label(RichText::new(format!("Login settings for {}", short_name(&c.target))).size(17.0).strong().color(TEXT));
                ui.label(RichText::new(format!("{} on {}", c.target, c.portal)).monospace().size(11.5).color(MUTED));
                ui.add_space(8.0);
                ui.radio_value(&mut c.chap, false, "No password: the server allows this computer by its iSCSI name");
                ui.radio_value(&mut c.chap, true, "User name and password (CHAP)");
                if c.chap {
                    ui.add_space(6.0);
                    egui::Grid::new("chapgrid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                        ui.label("User name");
                        ui.add(egui::TextEdit::singleline(&mut c.user).desired_width(260.0));
                        ui.end_row();
                        ui.label("Password");
                        ui.add(egui::TextEdit::singleline(&mut c.pass).password(true).hint_text("leave empty to keep the saved one").desired_width(260.0));
                        ui.end_row();
                    });
                    ui.label(RichText::new("Most servers need a CHAP password of 12 to 16 characters.").size(11.5).color(MUTED));
                    ui.add_space(4.0);
                    ui.checkbox(&mut c.mutual, "The server proves itself too (mutual CHAP)");
                    if c.mutual {
                        egui::Grid::new("chapgrid2").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                            ui.label("Server's user name");
                            ui.add(egui::TextEdit::singleline(&mut c.user_in).desired_width(240.0));
                            ui.end_row();
                            ui.label("Server's password");
                            ui.add(egui::TextEdit::singleline(&mut c.pass_in).password(true).hint_text("leave empty to keep").desired_width(240.0));
                            ui.end_row();
                        });
                    }
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let valid = !c.chap || !c.user.trim().is_empty();
                    let label = if c.connect_after { "Save and connect" } else { "Save" };
                    save = ui.add_enabled(valid, accent_button(label)).clicked();
                    close = ui.button("Cancel").clicked();
                });
            });
            if save {
                let c = self.chap.take().unwrap();
                let (t, p) = (c.target.as_str(), c.portal.as_str());
                let mut steps = vec![Self::update_step(t, p, "node.session.auth.authmethod", if c.chap { "CHAP" } else { "None" })];
                if c.chap {
                    steps.push(Self::update_step(t, p, "node.session.auth.username", c.user.trim()));
                    if !c.pass.is_empty() {
                        steps.push(Self::update_step(t, p, "node.session.auth.password", &c.pass).redact(&c.pass));
                    }
                    if c.mutual {
                        steps.push(Self::update_step(t, p, "node.session.auth.username_in", c.user_in.trim()));
                        if !c.pass_in.is_empty() {
                            steps.push(Self::update_step(t, p, "node.session.auth.password_in", &c.pass_in).redact(&c.pass_in));
                        }
                    } else {
                        steps.push(Self::update_step(t, p, "node.session.auth.username_in", ""));
                    }
                }
                if c.connect_after {
                    steps.push(Self::node_step(t, p, &["--login"]));
                }
                self.action(format!("Save login settings for {}", short_name(t)), steps);
            } else if close || resp.should_close() {
                self.chap = None;
            }
        }

        // Initiator name
        if self.initiator_edit.is_some() {
            let mut save = false;
            let mut close = false;
            let connected = !self.sessions.is_empty();
            let resp = egui::Modal::new(egui::Id::unique("iname")).show(ctx, |ui| {
                let v = self.initiator_edit.as_mut().unwrap();
                ui.set_width(480.0);
                ui.label(RichText::new("Change this computer's iSCSI name").size(17.0).strong().color(TEXT));
                ui.add_space(6.0);
                ui.label(RichText::new("Storage servers that allow computers by name will need the new name added. The iSCSI service restarts afterwards.").color(MUTED));
                ui.add_space(8.0);
                ui.add(egui::TextEdit::singleline(v).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                let valid = valid_iqn(v);
                if !valid {
                    ui.label(RichText::new("Use the form iqn.YYYY-MM.reversed.domain:name").size(12.0).color(BAD));
                }
                if connected {
                    ui.label(RichText::new("Disconnect all storage before changing the name.").size(12.0).color(WARN));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    save = ui.add_enabled(valid && !connected, accent_button("Save")).clicked();
                    if ui.button("Make a new one").clicked() {
                        *v = new_initiator_name();
                    }
                    close = ui.button("Cancel").clicked();
                });
            });
            if save {
                let name = self.initiator_edit.take().unwrap().trim().to_string();
                let script = "printf '## DO NOT EDIT OR REMOVE THIS FILE!\\n## If you remove this file, the iSCSI daemon will not start.\\nInitiatorName=%s\\n' \"$1\" > /etc/iscsi/initiatorname.iscsi";
                let steps = vec![
                    Step::owned(vec!["sh".into(), "-c".into(), script.into(), "sh".into(), name], Root::Yes),
                    Step::new(&["systemctl", "restart", "iscsid.service"], Root::Yes),
                ];
                self.action("Change iSCSI name", steps);
            } else if close || resp.should_close() {
                self.initiator_edit = None;
            }
        }
    }
}
