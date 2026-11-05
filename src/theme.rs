//! Palette pastello (niente bianco) e stile dell'interfaccia.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// Sfondo: azzurro pervinca.
pub const BG: Color32 = Color32::from_rgb(0xD6, 0xE3, 0xF8);
/// Pannelli, finestre e campi di testo: azzurro cielo chiaro (mai bianco).
pub const SURFACE: Color32 = Color32::from_rgb(0xE4, 0xEC, 0xFA);
/// Riga alternata delle tabelle: lilla.
pub const STRIPE: Color32 = Color32::from_rgb(0xE3, 0xDB, 0xF4);
pub const PRIMARY: Color32 = Color32::from_rgb(0x4F, 0x7C, 0xD6);
pub const PRIMARY_LIGHT: Color32 = Color32::from_rgb(0xC3, 0xD3, 0xF4);
/// Testo: blu notte.
pub const TEXT: Color32 = Color32::from_rgb(0x22, 0x31, 0x5E);
/// Testo secondario: blu polvere.
pub const MUTED: Color32 = Color32::from_rgb(0x58, 0x68, 0x98);
pub const BORDER: Color32 = Color32::from_rgb(0xA9, 0xBC, 0xE6);
pub const OK: Color32 = Color32::from_rgb(0x1F, 0x86, 0x5C);
/// Pulsanti principali: blu, menta e pesca pastello.
pub const SCAN: Color32 = Color32::from_rgb(0x9B, 0xBF, 0xF4);
pub const DOWNLOAD: Color32 = Color32::from_rgb(0x9C, 0xDD, 0xC1);
pub const INSTALL: Color32 = Color32::from_rgb(0xF6, 0xBF, 0x9C);
/// Testo sui pulsanti pastello: blu notte.
pub const ON_COLOR: Color32 = Color32::from_rgb(0x1B, 0x28, 0x55);
pub const LOG_BG: Color32 = Color32::from_rgb(0x34, 0x41, 0x72);
pub const LOG_INFO: Color32 = Color32::from_rgb(0xB5, 0xD2, 0xFF);
pub const LOG_OK: Color32 = Color32::from_rgb(0xA8, 0xEC, 0xCC);
pub const LOG_WARN: Color32 = Color32::from_rgb(0xFF, 0xE2, 0xA0);
pub const LOG_ERR: Color32 = Color32::from_rgb(0xFF, 0xB4, 0xC2);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::light();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.extreme_bg_color = SURFACE;
    v.faint_bg_color = STRIPE;
    v.code_bg_color = SURFACE;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = PRIMARY_LIGHT;
    v.selection.stroke = Stroke::new(1.0, PRIMARY);
    v.hyperlink_color = PRIMARY;
    let r = CornerRadius::same(9);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_fill = SURFACE;
    v.widgets.noninteractive.weak_bg_fill = SURFACE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = PRIMARY_LIGHT;
    v.widgets.inactive.weak_bg_fill = PRIMARY_LIGHT;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0xB4, 0xC8, 0xF2);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xB4, 0xC8, 0xF2);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, PRIMARY);
    v.widgets.active.bg_fill = Color32::from_rgb(0xA3, 0xBB, 0xEE);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0xA3, 0xBB, 0xEE);
    v.widgets.open.bg_fill = PRIMARY_LIGHT;
    v.widgets.open.weak_bg_fill = PRIMARY_LIGHT;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
    });
}
