mod app;
mod demo;
mod exec;
mod model;
mod pages;
mod ui;

use app::{IscsiApp, Options, Page};

fn main() -> eframe::Result {
    let mut opts = Options { screenshot: None, force_demo: false, page: Page::Connections };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => opts.screenshot = args.next().map(Into::into),
            "--demo" => opts.force_demo = true,
            "--page" => {
                if let Some(p) = args.next().as_deref().and_then(Page::from_name) {
                    opts.page = p;
                }
            }
            "-h" | "--help" => {
                println!(
                    "iSCSI Manager — manage iSCSI storage connections on Ubuntu Desktop\n\n\
                     Options:\n  --demo                 show a pretend storage network\n  \
                     --page <name>          start on connections, targets, discover, computer or activity\n  \
                     --screenshot <file>    draw ~120 frames, save the window as PNG and exit"
                );
                return Ok(());
            }
            _ => {}
        }
    }
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("iSCSI Manager")
            .with_app_id("iscsi-manager")
            .with_inner_size([1200.0, 780.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("iSCSI Manager", native, Box::new(move |cc| Ok(Box::new(IscsiApp::new(cc, opts)))))
}
