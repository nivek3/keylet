use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Frame, Layout, Margin, RichText, Sense, Stroke,
    TextStyle, Ui,
};

// Layout and palette follow macOS Passwords: a full-height gray sidebar, a light content
// area, white grouped cards, Apple's system gray scale and systemBlue accent. Purple
// tones stay excluded.
pub const SIDEBAR: Color32 = Color32::from_rgb(236, 236, 238);
pub const CANVAS: Color32 = Color32::from_rgb(245, 245, 247);
pub const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
pub const CONTENT: Color32 = CANVAS;
pub const PAGE_MARGIN: Margin = Margin::symmetric(24, 16);
/// Width kept clear for the window's traffic-light buttons.
pub const TRAFFIC_LIGHTS: f32 = 78.0;
pub const TOOLBAR_HEIGHT: f32 = 32.0;
pub const INK: Color32 = Color32::from_rgb(29, 29, 31);
pub const MUTED: Color32 = Color32::from_rgb(92, 92, 97);
pub const SIDEBAR_INK: Color32 = Color32::from_rgb(72, 72, 77);
pub const BORDER: Color32 = Color32::from_rgb(229, 229, 234); // systemGray5
pub const FIELD_BORDER: Color32 = Color32::from_rgb(134, 134, 140);
pub const ACCENT: Color32 = Color32::from_rgb(0, 122, 255); // systemBlue
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0, 108, 230);
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(0, 95, 205);
pub const ACCENT_INK: Color32 = Color32::from_rgb(255, 255, 255);
const ACCENT_SUBTLE_INK: Color32 = Color32::from_rgb(222, 236, 255);
pub const FOCUS: Color32 = ACCENT;
pub const SOFT: Color32 = Color32::from_rgb(242, 242, 247); // systemGray6
pub const SEARCH: Color32 = SURFACE;
pub const SELECTED: Color32 = Color32::from_rgb(229, 229, 234); // systemGray5
pub const DANGER: Color32 = Color32::from_rgb(215, 0, 21);
pub const DANGER_INK: Color32 = DANGER;
pub const DANGER_HOVER: Color32 = Color32::from_rgb(195, 0, 19);
pub const DANGER_PRESSED: Color32 = Color32::from_rgb(170, 0, 16);
pub const DANGER_SOFT: Color32 = Color32::from_rgb(255, 236, 236);
pub const WARNING_INK: Color32 = Color32::from_rgb(133, 79, 0);
pub const CONTROL: Color32 = SURFACE;
pub const SECONDARY: Color32 = SELECTED;
const SECONDARY_HOVER: Color32 = Color32::from_rgb(220, 220, 226);
const SECONDARY_PRESSED: Color32 = Color32::from_rgb(209, 209, 214); // systemGray4
pub const CONTROL_HOVER: Color32 = SOFT;
pub const TRANSPARENT: Color32 = Color32::TRANSPARENT;
const HOVER: Color32 = Color32::from_black_alpha(10);
const SHADOW: Color32 = Color32::from_black_alpha(28);
const CARD_RADIUS: u8 = 10;
const ROW_PADDING: egui::Vec2 = egui::vec2(14.0, 10.0);
/// A tile's vertical gradient, lighter at the top like the generated icons in
/// Passwords. A solid color has equal ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    top: Color32,
    bottom: Color32,
}

impl From<Color32> for Tile {
    fn from(color: Color32) -> Self {
        Self {
            top: color,
            bottom: color,
        }
    }
}

const fn tile(top: [u8; 3], bottom: [u8; 3]) -> Tile {
    Tile {
        top: Color32::from_rgb(top[0], top[1], top[2]),
        bottom: Color32::from_rgb(bottom[0], bottom[1], bottom[2]),
    }
}

/// Monogram tile colors, picked per wallet name like Passwords' generated icons:
/// Apple's system colors at the bottom, each lifted 18% toward white at the top. As
/// in Passwords the letter is decorative, since the wallet name sits beside it.
const TILES: [Tile; 8] = [
    tile([46, 146, 255], [0, 122, 255]),   // blue
    tile([85, 190, 209], [48, 176, 199]),  // teal
    tile([89, 209, 119], [52, 199, 89]),   // green
    tile([255, 168, 46], [255, 149, 0]),   // orange
    tile([255, 94, 85], [255, 59, 48]),    // red
    tile([255, 83, 116], [255, 45, 85]),   // pink
    tile([179, 154, 123], [162, 132, 94]), // brown
    tile([87, 188, 234], [50, 173, 230]),  // cyan
];
const CREATE_TILE: Tile = tile([46, 146, 255], [0, 122, 255]); // systemBlue
/// Fills a rounded rect with a tile's vertical gradient. egui fills shapes with one
/// color, so the rect is tessellated and each vertex recolored by its height; the
/// anti-aliasing fringe keeps its coverage through the vertex alpha.
fn paint_tile(painter: &egui::Painter, rect: egui::Rect, radius: f32, tile: Tile) {
    if tile.top == tile.bottom {
        painter.rect_filled(rect, radius, tile.top);
        return;
    }
    let mut mesh = egui::Mesh::default();
    egui::epaint::Tessellator::new(
        painter.pixels_per_point(),
        egui::epaint::TessellationOptions::default(),
        [1, 1],
        Vec::new(),
    )
    .tessellate_rect(
        &egui::epaint::RectShape::filled(rect, radius, Color32::WHITE),
        &mut mesh,
    );
    for vertex in &mut mesh.vertices {
        let t = ((vertex.pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
        let coverage = f32::from(vertex.color.a()) / 255.0;
        vertex.color = tile
            .top
            .lerp_to_gamma(tile.bottom, t)
            .gamma_multiply(coverage);
    }
    painter.add(egui::Shape::mesh(mesh));
}

const MEDIUM: &str = "medium";

pub fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Light);
    let mut fonts = egui::FontDefinitions::default();
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        fonts.font_data.insert(
            "macOS system".into(),
            egui::FontData::from_owned(bytes).into(),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "macOS system".into());
    }
    let mut medium = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    // SFNS.ttf only exposes its regular instance to egui; Helvetica Neue Medium
    // stands in for SF semibold titles and falls back to the system font.
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/HelveticaNeue.ttc") {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = 10;
        fonts.font_data.insert("macOS medium".into(), data.into());
        medium.insert(0, "macOS medium".into());
    }
    fonts
        .families
        .insert(FontFamily::Name(MEDIUM.into()), medium);
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.weak_text_color = Some(MUTED);
    style.visuals.panel_fill = CANVAS;
    style.visuals.extreme_bg_color = SURFACE;
    style.visuals.text_edit_bg_color = Some(SURFACE);
    style.visuals.faint_bg_color = SOFT;
    style.visuals.code_bg_color = SOFT;
    style.visuals.window_fill = SURFACE;
    style.visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.window_shadow.color = SHADOW;
    style.visuals.popup_shadow.color = SHADOW;
    style.visuals.menu_corner_radius = 8.into();
    style.visuals.window_corner_radius = 12.into();
    style.visuals.hyperlink_color = ACCENT;
    style.visuals.warn_fg_color = WARNING_INK;
    style.visuals.error_fg_color = DANGER_INK;
    style.visuals.text_cursor.stroke.color = ACCENT;
    style.visuals.selection.bg_fill = Color32::from_rgb(179, 215, 255);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, INK);
    style.visuals.widgets.noninteractive.fg_stroke.color = INK;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.inactive.bg_fill = CONTROL;
    style.visuals.widgets.inactive.weak_bg_fill = CONTROL;
    style.visuals.widgets.hovered.bg_fill = CONTROL_HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = CONTROL_HOVER;
    style.visuals.widgets.active.bg_fill = SELECTED;
    style.visuals.widgets.active.weak_bg_fill = SELECTED;
    style.visuals.widgets.open.bg_fill = SELECTED;
    style.visuals.widgets.open.weak_bg_fill = SELECTED;
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.open.fg_stroke.color = INK;
    style.visuals.widgets.inactive.fg_stroke.color = INK;
    style.visuals.widgets.hovered.fg_stroke.color = INK;
    style.visuals.widgets.active.fg_stroke.color = INK;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, FIELD_BORDER);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, FOCUS);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = 6.into();
        widget.expansion = 0.0;
    }
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.interact_size.y = 26.0;
    style.spacing.icon_width = 16.0;
    style
        .text_styles
        .insert(TextStyle::Heading, medium_font(20.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(12.0));
    ctx.set_style(style);
}

pub fn medium_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

fn button(
    ui: &mut Ui,
    label: &str,
    size: egui::Vec2,
    fills: [Color32; 3],
    ink: Color32,
) -> egui::Response {
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            for (visual, fill) in [
                (&mut widgets.inactive, fills[0]),
                (&mut widgets.hovered, fills[1]),
                (&mut widgets.active, fills[2]),
            ] {
                // macOS alert buttons: filled, borderless, softly rounded.
                visual.weak_bg_fill = fill;
                visual.bg_stroke = Stroke::NONE;
                visual.corner_radius = 8.into();
                visual.expansion = 0.0;
            }
            ui.spacing_mut().button_padding.x = 16.0;
            ui.add(
                egui::Button::new((
                    egui::Atom::grow(),
                    RichText::new(label).font(medium_font(13.0)).color(ink),
                    egui::Atom::grow(),
                ))
                .min_size(size),
            )
        })
        .inner;
    focus_ring(ui, response.rect, response.has_focus());
    response
}

pub fn primary(ui: &mut Ui, label: &str) -> egui::Response {
    button(
        ui,
        label,
        egui::vec2(80.0, 32.0),
        [ACCENT, ACCENT_HOVER, ACCENT_PRESSED],
        ACCENT_INK,
    )
}

pub fn secondary(ui: &mut Ui, label: &str) -> egui::Response {
    button(
        ui,
        label,
        egui::vec2(80.0, 32.0),
        [SECONDARY, SECONDARY_HOVER, SECONDARY_PRESSED],
        INK,
    )
}

pub fn danger(ui: &mut Ui, label: &str) -> egui::Response {
    button(
        ui,
        label,
        egui::vec2(80.0, 32.0),
        [DANGER, DANGER_HOVER, DANGER_PRESSED],
        ACCENT_INK,
    )
}

#[derive(Clone, Copy)]
pub(crate) enum WalletChoice {
    Create,
    Import,
}

/// The icon tile for creating or importing, shared by the choice cards and alerts.
pub(crate) fn choice_tile(choice: WalletChoice) -> Tile {
    match choice {
        WalletChoice::Create => CREATE_TILE,
        WalletChoice::Import => TILES[7],
    }
}

pub(crate) fn wallet_choice(ui: &mut Ui, width: f32, choice: WalletChoice) -> egui::Response {
    let swatch = choice_tile(choice);
    let (title, detail) = match choice {
        WalletChoice::Create => ("Create wallet", "New seed phrase"),
        WalletChoice::Import => ("Import wallet", "Existing seed phrase"),
    };
    let size = egui::vec2(width, 112.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    painter.rect(
        rect,
        CARD_RADIUS,
        if response.is_pointer_button_down_on() {
            SOFT
        } else {
            SURFACE
        },
        Stroke::new(
            1.0_f32,
            if response.hovered() {
                FIELD_BORDER
            } else {
                BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    let icon = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 34.0),
        egui::vec2(36.0, 36.0),
    );
    paint_tile(painter, icon, 9.0, swatch);
    let center = icon.center();
    let point = |x, y| center + egui::vec2(x, y);
    let stroke = Stroke::new(2.0_f32, ACCENT_INK);
    if matches!(choice, WalletChoice::Create) {
        painter.line_segment([point(-7.0, 0.0), point(7.0, 0.0)], stroke);
        painter.line_segment([point(0.0, -7.0), point(0.0, 7.0)], stroke);
    } else {
        painter.line_segment([point(0.0, -8.0), point(0.0, 3.0)], stroke);
        painter.line(
            vec![point(-5.0, -2.0), point(0.0, 3.0), point(5.0, -2.0)],
            stroke,
        );
        painter.line(
            vec![
                point(-8.0, 3.0),
                point(-8.0, 8.0),
                point(8.0, 8.0),
                point(8.0, 3.0),
            ],
            stroke,
        );
    }
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 68.0),
        egui::Align2::CENTER_CENTER,
        title,
        medium_font(13.0),
        INK,
    );
    painter.text(
        egui::pos2(rect.center().x, rect.top() + 86.0),
        egui::Align2::CENTER_CENTER,
        detail,
        FontId::proportional(11.0),
        MUTED,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    focus_ring(ui, rect, response.has_focus());
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Primary,
    Secondary,
    Danger,
}

/// A button in a `button_bar`.
#[derive(Clone, Copy)]
pub struct Action<'a> {
    label: &'a str,
    tone: Tone,
    enabled: bool,
}

impl<'a> Action<'a> {
    pub fn primary(label: &'a str) -> Self {
        Self {
            label,
            tone: Tone::Primary,
            enabled: true,
        }
    }

    pub fn secondary(label: &'a str) -> Self {
        Self {
            tone: Tone::Secondary,
            ..Self::primary(label)
        }
    }

    pub fn danger(label: &'a str) -> Self {
        Self {
            tone: Tone::Danger,
            ..Self::primary(label)
        }
    }

    pub fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }
}

/// The one button layout used everywhere, following macOS: a single row; extra
/// actions on the leading edge; Cancel and the default action on the trailing edge,
/// with the default last. Returns the clicks in the order the actions were given.
pub fn button_bar(
    ui: &mut Ui,
    leading: &[Action<'_>],
    trailing: &[Action<'_>],
) -> (Vec<bool>, Vec<bool>) {
    let draw = |ui: &mut Ui, action: &Action<'_>| {
        ui.add_enabled_ui(action.enabled, |ui| match action.tone {
            Tone::Primary => primary(ui, action.label),
            Tone::Secondary => secondary(ui, action.label),
            Tone::Danger => danger(ui, action.label),
        })
        .inner
        .clicked()
    };
    ui.horizontal(|ui| {
        let lead = leading.iter().map(|action| draw(ui, action)).collect();
        let mut trail = vec![false; trailing.len()];
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            for (index, action) in trailing.iter().enumerate().rev() {
                trail[index] = draw(ui, action);
            }
        });
        (lead, trail)
    })
    .inner
}

pub fn actions(ui: &mut Ui, back: &str, next: &str) -> (bool, bool) {
    ui.add_space(20.0);
    let (_, clicks) = button_bar(ui, &[], &[Action::secondary(back), Action::primary(next)]);
    (clicks[1], clicks[0])
}

pub fn backup_actions(ui: &mut Ui) -> (bool, bool, bool) {
    let (lead, trail) = button_bar(
        ui,
        &[Action::secondary("Skip & save")],
        &[Action::secondary("Cancel"), Action::primary("Save")],
    );
    (trail[1], lead[0], trail[0])
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(MUTED));
}

pub fn label(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(RichText::new(text).font(medium_font(12.0)))
}

/// Caption above a grouped card, aligned with the card's row text.
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(16.0);
    ui.horizontal(|ui| {
        ui.add_space(ROW_PADDING.x);
        ui.add(egui::Label::new(RichText::new(text).font(medium_font(13.0))).selectable(false));
    });
    ui.add_space(4.0);
}

pub fn footnote(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(ROW_PADDING.x);
        ui.add(egui::Label::new(RichText::new(text).size(11.0).color(MUTED)).wrap());
    })
    .response
}

/// A footnote with a trailing text action, such as "Hide". Returns its click.
pub fn footnote_action(ui: &mut Ui, text: &str, action: &str) -> egui::Response {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(ROW_PADDING.x);
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            ui.add_space(ROW_PADDING.x);
            let link = ui.add(
                egui::Button::new(RichText::new(action).size(12.0).color(ACCENT))
                    .frame(false)
                    .min_size(egui::vec2(0.0, 0.0)),
            );
            focus_ring(ui, link.rect, link.has_focus());
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.add(egui::Label::new(RichText::new(text).size(11.0).color(MUTED)).wrap());
            });
            link.on_hover_cursor(egui::CursorIcon::PointingHand)
        })
        .inner
    })
    .inner
}

pub fn page_header(ui: &mut Ui, title: &str, wallet: Option<&str>) -> egui::Response {
    let response = ui
        .horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(title).heading()).wrap());
            if let Some(wallet) = wallet {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    Frame::new()
                        .fill(SELECTED)
                        .corner_radius(6)
                        .inner_margin(Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(wallet).size(12.0)).truncate());
                        });
                });
            }
        })
        .response;
    ui.add_space(4.0);
    response
}

/// Large icon and title at the top of a wallet's details, as in Passwords.
pub fn detail_header(ui: &mut Ui, name: &str, subtitle: &str) -> egui::Response {
    ui.horizontal(|ui| {
        monogram(ui, name, 56.0);
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.add(egui::Label::new(RichText::new(name).font(medium_font(22.0))).wrap());
            ui.label(RichText::new(subtitle).size(12.0).color(MUTED));
        });
    })
    .response
}

pub fn input_frame() -> Frame {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, FIELD_BORDER))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(9, 6))
}

pub fn public_field(ui: &mut Ui, title: &str, text: &mut String, limit: usize) {
    let label = label(ui, title);
    let field = input_frame().show(ui, |ui| {
        ui.add(
            egui::TextEdit::singleline(text)
                .char_limit(limit)
                .frame(false)
                .margin(Margin::ZERO)
                .desired_width(f32::INFINITY),
        )
        .labelled_by(label.id)
    });
    focus_ring(ui, field.response.rect, field.inner.has_focus());
    ui.add_space(4.0);
}

/// A 64pt rounded tile with a glyph and an optional corner badge, like the app icon
/// with an overlay that macOS alerts show.
pub fn alert_icon(ui: &mut Ui, tile: impl Into<Tile>, glyph: &str, badge: Option<Color32>) {
    let size = 64.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let painter = ui.painter();
    paint_tile(painter, rect, size * 0.225, tile.into());
    painter.text(
        rect.center() + egui::vec2(0.0, 1.0),
        egui::Align2::CENTER_CENTER,
        glyph,
        medium_font(30.0),
        ACCENT_INK,
    );
    if let Some(badge) = badge {
        let center = rect.right_bottom() - egui::vec2(6.0, 6.0);
        painter.circle(center, 12.0, badge, Stroke::new(2.5_f32, CANVAS));
        painter.line_segment(
            [center - egui::vec2(5.0, 0.0), center + egui::vec2(5.0, 0.0)],
            Stroke::new(2.5_f32, ACCENT_INK),
        );
    }
}

/// Icon for a wallet: its monogram tile and letter.
pub fn wallet_alert_icon(ui: &mut Ui, name: &str, badge: Option<Color32>) {
    let letter = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('K')
        .to_uppercase()
        .to_string();
    alert_icon(ui, tile_color(name), &letter, badge);
}

/// macOS alert layout: the icon on the left; a bold title, body text and the
/// alert's own controls on the right.
pub fn alert<R>(
    ui: &mut Ui,
    icon: impl FnOnce(&mut Ui),
    title: &str,
    body: &str,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        icon(ui);
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.add(egui::Label::new(RichText::new(title).font(medium_font(15.0))).wrap());
            ui.add(egui::Label::new(RichText::new(body).size(13.0)).wrap());
            ui.add_space(4.0);
            add(ui)
        })
        .inner
    })
    .inner
}

/// Right-aligned alert buttons: the secondary choice, then the default one at the
/// trailing edge. Returns `(primary, secondary)` clicks.
pub fn alert_buttons(
    ui: &mut Ui,
    secondary_label: &str,
    primary_label: &str,
    destructive: bool,
    primary_enabled: bool,
) -> (bool, bool) {
    ui.add_space(8.0);
    let default = if destructive {
        Action::danger(primary_label)
    } else {
        Action::primary(primary_label)
    };
    let (_, clicks) = button_bar(
        ui,
        &[],
        &[
            Action::secondary(secondary_label),
            default.enabled(primary_enabled),
        ],
    );
    (clicks[1], clicks[0])
}

/// A bordered single-line field for use inside a `form_row`, at most `width` wide.
pub fn text_field(ui: &mut Ui, value: &mut String, limit: usize, width: f32) -> egui::Response {
    let frame = input_frame();
    let chrome = frame.total_margin().sum().x;
    let width = width.min(ui.available_width() - chrome).max(40.0);
    let field = frame.show(ui, |ui| {
        ui.set_width(width);
        ui.add(
            egui::TextEdit::singleline(value)
                .char_limit(limit)
                .frame(false)
                .margin(Margin::ZERO)
                .desired_width(f32::INFINITY),
        )
    });
    focus_ring(ui, field.response.rect, field.inner.has_focus());
    field.inner
}

/// White rounded card holding rows, like a grouped form section.
pub fn group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(CARD_RADIUS)
        .inner_margin(Margin::same(1))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
}

pub fn group_separator(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(
        (rect.left() + ROW_PADDING.x)..=rect.right(),
        rect.center().y,
        Stroke::new(1.0_f32, BORDER),
    );
}

#[derive(Clone, Copy)]
pub enum RowValue<'a> {
    Mono(&'a str),
    Text(&'a str),
    Muted(&'a str),
}

/// One row of a grouped card: a caption on the left, a wrapping value, and an optional
/// trailing action that brightens while the row is hovered.
pub fn field_row(
    ui: &mut Ui,
    label: &str,
    value: RowValue<'_>,
    trailing: Option<&str>,
) -> egui::Response {
    let width = ui.available_width();
    let label_width = (width * 0.26).clamp(72.0, 104.0);
    let trailing = trailing.map(|text| {
        ui.painter().layout_no_wrap(
            text.to_owned(),
            FontId::proportional(12.0),
            Color32::PLACEHOLDER,
        )
    });
    let trailing_width = trailing.as_ref().map_or(0.0, |g| g.size().x + 12.0);
    let value_width = (width - 2.0 * ROW_PADDING.x - label_width - trailing_width).max(40.0);
    let (text, font, color) = match value {
        RowValue::Mono(text) => (text, FontId::monospace(12.0), INK),
        RowValue::Text(text) => (text, FontId::proportional(13.0), INK),
        RowValue::Muted(text) => (text, FontId::proportional(13.0), MUTED),
    };
    let value = ui
        .painter()
        .layout(text.to_owned(), font, color, value_width);
    let caption = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(13.0), MUTED);
    // Center the caption on the value's first line so mixed fonts share a midline.
    let first_line = value
        .rows
        .first()
        .map_or(caption.size().y, |row| row.height());
    let line = first_line.max(caption.size().y).max(18.0);
    let value_offset = (line - first_line) / 2.0;
    let height = (value_offset + value.size().y).max(line) + 2.0 * ROW_PADDING.y;
    // Only rows with a trailing action are buttons; the rest are plain read-only rows.
    let interactive = trailing.is_some();
    let sense = if interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    let painter = ui.painter();
    if interactive && response.hovered() {
        painter.rect_filled(rect.shrink(3.0), 7, HOVER);
    }
    let top = rect.top() + ROW_PADDING.y;
    let caption_y = top + (line - caption.size().y) / 2.0;
    painter.galley(
        egui::pos2(rect.left() + ROW_PADDING.x, caption_y),
        caption,
        MUTED,
    );
    painter.galley(
        egui::pos2(
            rect.left() + ROW_PADDING.x + label_width,
            top + value_offset,
        ),
        value,
        color,
    );
    if let Some(trailing) = trailing {
        let pos = egui::pos2(
            rect.right() - ROW_PADDING.x - trailing.size().x,
            top + (line - trailing.size().y) / 2.0,
        );
        let tint = if response.hovered() || response.has_focus() {
            ACCENT
        } else {
            MUTED
        };
        painter.galley(pos, trailing, tint);
    }
    if !interactive {
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, label));
        return response;
    }
    focus_ring(ui, rect.shrink(1.0), response.has_focus());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A full-width text action row, like "Delete Password" in Passwords.
pub fn action_row(ui: &mut Ui, label: &str, color: Color32) -> egui::Response {
    let width = ui.available_width();
    let sense = if ui.is_enabled() {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, 18.0 + 2.0 * ROW_PADDING.y), sense);
    if response.hovered() && ui.is_enabled() {
        ui.painter().rect_filled(rect.shrink(3.0), 7, HOVER);
    }
    ui.painter().text(
        egui::pos2(rect.left() + ROW_PADDING.x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        if ui.is_enabled() {
            color
        } else {
            color.gamma_multiply(0.45)
        },
    );
    focus_ring(ui, rect.shrink(1.0), response.has_focus());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn destructive_row(ui: &mut Ui, label: &str) -> egui::Response {
    action_row(ui, label, DANGER_INK)
}

/// Caption column on the left and a control on the right, aligned with `field_row`.
pub fn form_row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let label_width = (ui.available_width() * 0.26).clamp(72.0, 104.0);
    Frame::new()
        .inner_margin(Margin::symmetric(ROW_PADDING.x as i8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Controls in these rows are about 28pt tall; give the caption the
                // same height so the row's center alignment lines them up.
                let height = 28.0;
                ui.set_min_height(height);
                ui.allocate_ui_with_layout(
                    egui::vec2(label_width - ui.spacing().item_spacing.x, height),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_size(egui::vec2(
                            label_width - ui.spacing().item_spacing.x,
                            height,
                        ));
                        ui.add(
                            egui::Label::new(RichText::new(label).size(13.0).color(MUTED))
                                .selectable(false)
                                .truncate(),
                        );
                    },
                );
                add(ui)
            })
            .inner
        })
        .inner
}

pub fn toolbar_button(ui: &mut Ui, label: &str) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.hovered.weak_bg_fill = HOVER;
            widgets.active.weak_bg_fill = Color32::from_black_alpha(20);
            widgets.hovered.bg_stroke = Stroke::NONE;
            widgets.active.bg_stroke = Stroke::NONE;
            ui.add(
                egui::Button::new(RichText::new(label).size(17.0).color(SIDEBAR_INK))
                    .min_size(egui::vec2(28.0, 28.0))
                    .frame_when_inactive(false),
            )
        })
        .inner;
    focus_ring(ui, response.rect, response.has_focus());
    response
}

fn labeled(ui: &Ui, response: egui::Response, label: &str) -> egui::Response {
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}

/// Line weight shared by the toolbar glyphs so they read as one set.
const TOOLBAR_STROKE: f32 = 1.5;
pub fn add_button(ui: &mut Ui) -> egui::Response {
    let response = toolbar_button(ui, "");
    let center = response.rect.center();
    let stroke = Stroke::new(TOOLBAR_STROKE, SIDEBAR_INK);
    ui.painter().line_segment(
        [center - egui::vec2(6.5, 0.0), center + egui::vec2(6.5, 0.0)],
        stroke,
    );
    ui.painter().line_segment(
        [center - egui::vec2(0.0, 6.5), center + egui::vec2(0.0, 6.5)],
        stroke,
    );
    labeled(ui, response, "Add wallet")
}

pub fn more_button(ui: &mut Ui) -> egui::Response {
    let response = toolbar_button(ui, "");
    let center = response.rect.center();
    let stroke = Stroke::new(TOOLBAR_STROKE, SIDEBAR_INK);
    ui.painter().circle_stroke(center, 8.0, stroke);
    for x in [-3.75, 0.0, 3.75] {
        ui.painter()
            .circle_filled(center + egui::vec2(x, 0.0), 1.3, SIDEBAR_INK);
    }
    labeled(ui, response, "More actions")
}

pub fn search_icon(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::hover());
    paint_search_icon(ui, rect, MUTED);
}

pub fn search_button(ui: &mut Ui) -> egui::Response {
    let response = toolbar_button(ui, "");
    paint_search_icon(
        ui,
        egui::Rect::from_center_size(
            response.rect.center() + egui::vec2(1.0, 1.0),
            egui::vec2(16.0, 16.0),
        ),
        SIDEBAR_INK,
    );
    labeled(ui, response, "Search wallets")
}

fn paint_search_icon(ui: &Ui, rect: egui::Rect, color: Color32) {
    let center = rect.center() - egui::vec2(2.0, 2.0);
    let stroke = Stroke::new(TOOLBAR_STROKE, color);
    ui.painter().circle_stroke(center, 4.5, stroke);
    ui.painter().line_segment(
        [
            center + egui::vec2(3.5, 3.5),
            rect.right_bottom() - egui::vec2(1.5, 1.5),
        ],
        stroke,
    );
}

pub fn navigation_button(ui: &mut Ui, label: &str, sidebar: Option<bool>) -> egui::Response {
    let response = toolbar_button(ui, "");
    let stroke = Stroke::new(TOOLBAR_STROKE, SIDEBAR_INK);
    let painter = ui.painter();
    if sidebar.is_some() {
        let rect = egui::Rect::from_center_size(response.rect.center(), egui::vec2(19.0, 15.0));
        painter.rect_stroke(rect, 3.5, stroke, egui::StrokeKind::Inside);
        painter.line_segment(
            [
                egui::pos2(rect.left() + 6.5, rect.top() + 1.0),
                egui::pos2(rect.left() + 6.5, rect.bottom() - 1.0),
            ],
            stroke,
        );
    } else {
        let tint = if ui.is_enabled() {
            SIDEBAR_INK
        } else {
            Color32::from_rgb(190, 190, 195)
        };
        let stroke = Stroke::new(1.8_f32, tint);
        let left = egui::pos2(response.rect.center().x - 3.0, response.rect.center().y);
        painter.line_segment([left, left + egui::vec2(6.0, -6.0)], stroke);
        painter.line_segment([left, left + egui::vec2(6.0, 6.0)], stroke);
    }
    labeled(ui, response, label)
}

fn tile_color(name: &str) -> Tile {
    let hash = name.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    TILES[hash as usize % TILES.len()]
}

/// Rounded-square monogram, the Passwords fallback for sites without an icon.
pub fn monogram(ui: &mut Ui, name: &str, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    paint_monogram(ui, rect, name);
}

fn paint_monogram(ui: &Ui, rect: egui::Rect, name: &str) {
    let painter = ui.painter();
    paint_tile(painter, rect, rect.width() * 0.225, tile_color(name));
    let letter = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('K')
        .to_uppercase()
        .to_string();
    painter.text(
        rect.center() + egui::vec2(0.0, rect.height() * 0.02),
        egui::Align2::CENTER_CENTER,
        letter,
        medium_font(rect.height() * 0.5),
        ACCENT_INK,
    );
}

pub const WALLET_ROW_HEIGHT: f32 = 44.0;
pub fn wallet_row(ui: &mut Ui, name: &str, subtitle: &str, selected: bool) -> egui::Response {
    let width = ui.available_width();
    let (rect, row) = ui.allocate_exact_size(egui::vec2(width, WALLET_ROW_HEIGHT), Sense::click());
    let painter = ui.painter();
    // Like macOS lists, the selection is blue only while the window is active.
    let active = selected && ui.ctx().input(|input| input.focused);
    if active {
        painter.rect_filled(rect, 7, ACCENT);
    } else if selected {
        painter.rect_filled(rect, 7, SELECTED);
    } else if row.hovered() {
        painter.rect_filled(rect, 7, HOVER);
    }
    let icon = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 8.0, rect.center().y - 14.0),
        egui::vec2(28.0, 28.0),
    );
    paint_monogram(ui, icon, name);
    let text_left = icon.right() + 9.0;
    let text_width = (rect.right() - 8.0 - text_left).max(0.0);
    let (title_ink, subtitle_ink) = if active {
        (ACCENT_INK, ACCENT_SUBTLE_INK)
    } else {
        (INK, MUTED)
    };
    let truncated = |text: &str, font: FontId, color| {
        let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
        job.wrap = egui::text::TextWrapping::truncate_at_width(text_width);
        ui.painter().layout_job(job)
    };
    let title = truncated(name, FontId::proportional(13.0), title_ink);
    let subtitle = truncated(subtitle, FontId::proportional(11.0), subtitle_ink);
    let block = title.size().y + subtitle.size().y + 1.0;
    let top = rect.center().y - block / 2.0;
    painter.galley(
        egui::pos2(text_left, top),
        std::sync::Arc::clone(&title),
        title_ink,
    );
    painter.galley(
        egui::pos2(text_left, top + title.size().y + 1.0),
        subtitle,
        subtitle_ink,
    );
    focus_ring(ui, rect, row.has_focus());
    row.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            name,
        )
    });
    row.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `0xad12…c38b10`: the prefix (with `0x` when present) plus four characters, then
/// the last six. Used wherever an address is shown before the person asks for it.
pub fn abbreviate_address(address: &str) -> String {
    let chars: Vec<char> = address.chars().collect();
    let head = if address.starts_with("0x") { 6 } else { 4 };
    let tail = 6;
    if chars.len() <= head + tail + 1 {
        return address.to_owned();
    }
    let head: String = chars[..head].iter().collect();
    let tail: String = chars[chars.len() - tail..].iter().collect();
    format!("{head}…{tail}")
}

pub fn focus_ring(ui: &Ui, rect: egui::Rect, focused: bool) {
    if focused {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(2.0_f32, FOCUS.gamma_multiply(0.7)),
            egui::StrokeKind::Outside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_wallet_choices_fit_side_by_side_at_narrow_width() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(292.0, 200.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(Frame::new().inner_margin(16))
                    .show(ctx, |ui| {
                        let bounds = ui.max_rect();
                        let width =
                            ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).floor();
                        ui.horizontal(|ui| {
                            for choice in [WalletChoice::Create, WalletChoice::Import] {
                                let card = wallet_choice(ui, width, choice);
                                assert!(bounds.contains_rect(card.rect));
                                assert!((card.rect.width() - width).abs() < 0.5);
                            }
                        });
                    });
            },
        );
    }

    #[test]
    fn palette_keeps_text_and_focus_legible() {
        fn luminance(color: Color32) -> f32 {
            [color.r(), color.g(), color.b()]
                .into_iter()
                .zip([0.2126, 0.7152, 0.0722])
                .map(|(channel, weight)| {
                    let value = f32::from(channel) / 255.0;
                    weight
                        * if value <= 0.04045 {
                            value / 12.92
                        } else {
                            ((value + 0.055) / 1.055).powf(2.4)
                        }
                })
                .sum()
        }
        let contrast = |a, b| {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        };
        for surface in [
            SIDEBAR,
            CANVAS,
            SURFACE,
            CONTENT,
            SOFT,
            SELECTED,
            CONTROL,
            CONTROL_HOVER,
        ] {
            assert!(contrast(INK, surface) >= 4.5);
            assert!(contrast(MUTED, surface) >= 4.5);
            assert!(contrast(FOCUS, surface) >= 3.0);
        }
        // systemBlue itself measures 4.0:1 under white, as on Apple's own buttons.
        for fill in [ACCENT, ACCENT_HOVER, ACCENT_PRESSED] {
            assert!(contrast(ACCENT_INK, fill) >= 4.0);
        }
        for fill in [DANGER, DANGER_HOVER, DANGER_PRESSED] {
            assert!(contrast(ACCENT_INK, fill) >= 4.5);
        }
        // Monogram letters are decorative, as on Apple's system-color tiles; the floor
        // only stops a tile from washing out the white letter entirely.
        for tile in TILES.iter().chain([&CREATE_TILE]) {
            assert!(contrast(ACCENT_INK, tile.top) >= 1.8);
            assert!(contrast(ACCENT_INK, tile.bottom) >= 2.0);
        }
        assert!(contrast(ACCENT_SUBTLE_INK, ACCENT) >= 3.0);
        assert!(contrast(DANGER_INK, DANGER_SOFT) >= 4.5);
        for surface in [CANVAS, SEARCH] {
            assert!(contrast(FIELD_BORDER, surface) >= 3.0);
        }
        assert!(contrast(WARNING_INK, SURFACE) >= 4.5);
        assert!(contrast(DANGER_INK, SURFACE) >= 4.5);
        assert!(contrast(FOCUS, SURFACE) >= 3.0);
    }

    #[test]
    fn page_heading_wraps_long_names_without_overflow() {
        for width in [280.0, 452.0] {
            let ctx = egui::Context::default();
            configure(&ctx);
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width + 32.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(Frame::new().inner_margin(16))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            for name in [
                                "demo",
                                "long-wallet-name-with-no-spaces-".repeat(4).as_str(),
                                "我的钱包",
                            ] {
                                let heading = page_header(ui, name, None);
                                assert!(bounds.contains_rect(heading.rect));
                                if name.len() > 64 {
                                    assert!(heading.rect.height() > 30.0);
                                }
                            }
                        });
                },
            );
        }
    }

    #[test]
    fn wallet_rows_fit_sidebar_and_narrow_window() {
        for width in [124.0, 164.0, 192.0, 236.0, 364.0] {
            let ctx = egui::Context::default();
            configure(&ctx);
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width + 32.0, 200.0),
                )),
                ..Default::default()
            };
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default()
                    .frame(Frame::new().inner_margin(16))
                    .show(ctx, |ui| {
                        let bounds = ui.max_rect();
                        let row = wallet_row(ui, &"long-wallet-".repeat(5), "0xf39F…2266", true);
                        assert!(bounds.contains_rect(row.rect));
                        assert!(row.rect.width() <= width + 1.0);
                        assert!((row.rect.height() - WALLET_ROW_HEIGHT).abs() < 0.5);
                    });
            });
        }
    }

    #[test]
    fn wallet_row_can_be_activated_with_the_keyboard() {
        {
            let ctx = egui::Context::default();
            configure(&ctx);
            for activate in [false, true] {
                let mut input = egui::RawInput::default();
                if activate {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let row = wallet_row(ui, "Public fixture", "Not synced", false);
                        if activate {
                            assert!(row.has_focus());
                            assert!(row.clicked());
                        } else {
                            row.request_focus();
                        }
                    });
                });
            }
        }
    }

    #[test]
    fn addresses_abbreviate_to_four_and_six_characters() {
        assert_eq!(
            abbreviate_address("0xad123ab6983b7568b117621db94c34b8dfc38b10"),
            "0xad12…c38b10"
        );
        assert_eq!(
            abbreviate_address("HeFJTg9Z4zCrBfvmQ4FftpkjvShG8uYVDJr8Ud4htxbj"),
            "HeFJ…4htxbj"
        );
        assert_eq!(abbreviate_address("0x1234567890a"), "0x1234567890a");
        assert_eq!(abbreviate_address("short"), "short");
    }

    #[test]
    fn grouped_rows_wrap_addresses_inside_the_card() {
        for width in [280.0, 404.0, 520.0] {
            let ctx = egui::Context::default();
            configure(&ctx);
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width + 32.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(Frame::new().inner_margin(16))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            let card = group(ui, |ui| {
                                let eth = field_row(
                                    ui,
                                    "Ethereum",
                                    RowValue::Mono("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                                    Some("Copy"),
                                );
                                group_separator(ui);
                                let phrase = field_row(
                                    ui,
                                    "Seed phrase",
                                    RowValue::Text("••••••••••••"),
                                    Some("Show"),
                                );
                                group_separator(ui);
                                let delete = destructive_row(ui, "Delete wallet…");
                                [eth.rect, phrase.rect, delete.rect]
                            });
                            assert!(bounds.contains_rect(card.response.rect));
                            for row in card.inner {
                                assert!(card.response.rect.contains_rect(row));
                            }
                            if width >= 520.0 {
                                assert!(card.inner[0].height() < 44.0);
                            }
                        });
                },
            );
        }
    }

    #[test]
    fn field_rows_without_an_action_are_not_buttons() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let plain = field_row(ui, "Ethereum", RowValue::Muted("Not synced"), None);
                assert!(!plain.sense.senses_click());
                assert!(!plain.sense.is_focusable());
                let action = field_row(ui, "Seed phrase", RowValue::Text("••••"), Some("Show"));
                assert!(action.sense.senses_click());
            });
        });
    }

    #[test]
    fn system_font_and_form_render_without_overflow() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(440.0, 560.0),
            )),
            ..Default::default()
        };
        let mut text = "public form fixture".to_owned();
        let _ = ctx.run(raw, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let bounds = ui.max_rect();
                ui.set_max_width(380.0);
                ui.heading("Name your new wallet");
                public_field(ui, "Wallet name", &mut text, 64);
                let button = primary(ui, "Continue");
                assert!(bounds.contains_rect(button.rect));
                assert!(button.rect.width() <= 200.0);
                assert!(button.rect.height() <= 36.0);
            });
        });
    }
}
