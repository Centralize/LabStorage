//! Colours, style and small drawing helpers.

use egui::{Align, Color32, CornerRadius, Layout, Margin, RichText, Stroke, Vec2};
use std::time::Instant;

pub const BG: Color32 = Color32::from_rgb(0x13, 0x17, 0x1e);
pub const SIDE: Color32 = Color32::from_rgb(0x18, 0x1d, 0x26);
pub const CARD: Color32 = Color32::from_rgb(0x1f, 0x25, 0x31);
pub const CARD_HI: Color32 = Color32::from_rgb(0x26, 0x2d, 0x3b);
pub const LINE: Color32 = Color32::from_rgb(0x2e, 0x36, 0x46);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xea, 0xf0);
pub const MUTED: Color32 = Color32::from_rgb(0x8d, 0x97, 0xa9);
pub const ACCENT: Color32 = Color32::from_rgb(0x3f, 0xb6, 0xa8);
pub const GOOD: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
pub const WARN: Color32 = Color32::from_rgb(0xe3, 0xb3, 0x41);
pub const BAD: Color32 = Color32::from_rgb(0xe5, 0x5b, 0x52);

pub fn setup_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, LINE);
    v.extreme_bg_color = Color32::from_rgb(0x10, 0x13, 0x19);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.6);
    v.hyperlink_color = ACCENT;
    v.widgets.inactive.weak_bg_fill = CARD_HI;
    v.widgets.inactive.bg_fill = CARD_HI;
    v.widgets.inactive.corner_radius = CornerRadius::same(6);
    v.widgets.hovered.corner_radius = CornerRadius::same(6);
    v.widgets.active.corner_radius = CornerRadius::same(6);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x30, 0x38, 0x49);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.7));
    ctx.set_visuals_of(egui::Theme::Dark, v);
    ctx.all_styles_mut(|s| {
        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.5));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(13.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(21.0));
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(12.0, 5.0);
        s.spacing.interact_size.y = 26.0;
    });
}

pub fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let r = egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner;
    ui.add_space(10.0);
    r
}

pub fn note(ui: &mut egui::Ui, color: Color32, text: &str, extra: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.5)))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                dot(ui, color, 4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    extra(ui);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(text).color(TEXT)).wrap());
                    });
                });
            });
        });
    ui.add_space(8.0);
}

pub fn page_header(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.label(RichText::new(title).size(22.0).strong().color(TEXT));
    ui.label(RichText::new(sub).color(MUTED));
    ui.add_space(10.0);
}

pub fn section_title(ui: &mut egui::Ui, t: &str) {
    ui.label(RichText::new(t).size(15.5).strong().color(TEXT));
}

pub fn dot(ui: &mut egui::Ui, color: Color32, r: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(r * 2.0 + 4.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), r, color);
}

pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(color.lerp_to_gamma(Color32::WHITE, 0.25)));
        });
}

pub fn fact(ui: &mut egui::Ui, k: &str, v: &str) {
    ui.label(RichText::new(k).size(12.0).color(MUTED));
    ui.label(RichText::new(v).color(TEXT));
    ui.add_space(10.0);
}

pub fn accent_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(Color32::from_rgb(0x06, 0x1a, 0x17)).strong())
        .fill(ACCENT)
        .min_size(Vec2::new(0.0, 30.0))
}

pub fn danger_button(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_string()).color(Color32::from_rgb(0xff, 0xd6, 0xd2)))
        .fill(Color32::from_rgb(0x5a, 0x24, 0x22))
        .stroke(Stroke::new(1.0, Color32::from_rgb(0x84, 0x36, 0x32)))
        .min_size(Vec2::new(0.0, 30.0))
}

pub fn logo(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(40.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(10), ACCENT.gamma_multiply(0.18));
    let c = rect.center();
    // three stacked "disks" joined by a cable
    for i in 0..3 {
        let y = c.y - 9.0 + i as f32 * 8.0;
        let r = egui::Rect::from_center_size(egui::pos2(c.x - 3.0, y), Vec2::new(18.0, 6.0));
        p.rect_filled(r, CornerRadius::same(2), ACCENT);
        p.circle_filled(egui::pos2(r.max.x - 3.5, y), 1.3, BG);
    }
    p.line_segment([egui::pos2(c.x + 6.0, c.y - 9.0), egui::pos2(c.x + 12.0, c.y - 9.0)], Stroke::new(1.6, ACCENT));
    p.line_segment([egui::pos2(c.x + 12.0, c.y - 9.0), egui::pos2(c.x + 12.0, c.y + 10.0)], Stroke::new(1.6, ACCENT));
    p.circle_filled(egui::pos2(c.x + 12.0, c.y + 11.0), 2.5, ACCENT);
}

pub fn disk_icon(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 16.0), egui::Sense::hover());
    let p = ui.painter();
    let r = egui::Rect::from_center_size(rect.center(), Vec2::new(18.0, 12.0));
    p.rect_stroke(r, CornerRadius::same(3), Stroke::new(1.4, ACCENT), egui::StrokeKind::Inside);
    p.circle_filled(egui::pos2(r.max.x - 4.5, r.center().y), 1.6, ACCENT);
    p.line_segment([egui::pos2(r.min.x + 3.0, r.center().y), egui::pos2(r.min.x + 9.0, r.center().y)], Stroke::new(1.2, ACCENT));
}

pub fn ago(t: Instant) -> String {
    let s = t.elapsed().as_secs();
    match s {
        0..=4 => "just now".into(),
        5..=59 => format!("{s} s ago"),
        60..=3599 => format!("{} min ago", s / 60),
        _ => format!("{} h ago", s / 3600),
    }
}

pub fn new_initiator_name() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut x = (nanos as u64) ^ ((nanos >> 64) as u64) ^ ((std::process::id() as u64) << 32) ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    format!("iqn.2004-10.com.ubuntu:01:{:012x}", x & 0xffff_ffff_ffff)
}
