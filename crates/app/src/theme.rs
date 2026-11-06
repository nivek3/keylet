use eframe::egui::{
    self, Align, Color32, FontId, Frame, Layout, Margin, RichText, Stroke, TextStyle, Ui,
};

// Hallmark · v2.0.0 · theme: Dusk (by request) · scope: app · redesign: restyle
// Light native surfaces use warm neutrals; the user requested no purple layers or accents.
pub const CANVAS: Color32 = Color32::from_rgb(248, 247, 245);
pub const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
pub const CONTENT: Color32 = Color32::from_rgb(253, 252, 249);
pub const PAGE_MARGIN: Margin = Margin::symmetric(24, 16);
pub const INK: Color32 = Color32::from_rgb(41, 38, 34);
pub const MUTED: Color32 = Color32::from_rgb(105, 100, 94);
pub const SIDEBAR_INK: Color32 = Color32::from_rgb(56, 52, 47);
pub const BORDER: Color32 = Color32::from_rgb(227, 224, 218);
pub const FIELD_BORDER: Color32 = Color32::from_rgb(136, 130, 121);
pub const ACCENT: Color32 = Color32::from_rgb(48, 45, 41);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(66, 61, 56);
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(35, 32, 29);
pub const ACCENT_INK: Color32 = Color32::from_rgb(255, 253, 248);
pub const FOCUS: Color32 = Color32::from_rgb(83, 80, 75);
pub const SOFT: Color32 = CANVAS;
pub const SEARCH: Color32 = SURFACE;
pub const SELECTED: Color32 = Color32::from_rgb(235, 232, 225);
const AVATAR: Color32 = Color32::from_rgb(237, 233, 225);
pub const DANGER: Color32 = Color32::from_rgb(244, 186, 135);
pub const DANGER_INK: Color32 = Color32::from_rgb(134, 76, 24);
pub const DANGER_HOVER: Color32 = Color32::from_rgb(255, 208, 165);
pub const DANGER_PRESSED: Color32 = Color32::from_rgb(221, 164, 113);
pub const WARNING_SOFT: Color32 = Color32::from_rgb(255, 242, 225);
pub const WARNING_INK: Color32 = DANGER_INK;
pub const DANGER_SOFT: Color32 = WARNING_SOFT;
pub const RECOVERY_BODY_CORNERS: egui::CornerRadius = egui::CornerRadius {
    nw: 0,
    ne: 0,
    sw: 10,
    se: 10,
};
pub const CONTROL: Color32 = SURFACE;
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(246, 243, 238);
pub const TRANSPARENT: Color32 = Color32::TRANSPARENT;
const SHADOW: Color32 = Color32::from_black_alpha(18);

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
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "macOS system".into());
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.weak_text_color = Some(MUTED);
    style.visuals.panel_fill = CANVAS;
    style.visuals.extreme_bg_color = CANVAS;
    style.visuals.text_edit_bg_color = Some(CANVAS);
    style.visuals.faint_bg_color = SOFT;
    style.visuals.code_bg_color = SOFT;
    style.visuals.window_fill = SURFACE;
    style.visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.window_shadow.color = SHADOW;
    style.visuals.popup_shadow.color = SHADOW;
    style.visuals.hyperlink_color = FOCUS;
    style.visuals.warn_fg_color = WARNING_INK;
    style.visuals.error_fg_color = DANGER_INK;
    style.visuals.text_cursor.stroke.color = FOCUS;
    style.visuals.selection.bg_fill = SELECTED;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, INK);
    style.visuals.widgets.noninteractive.fg_stroke.color = INK;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.inactive.bg_fill = CONTROL;
    style.visuals.widgets.inactive.weak_bg_fill = CONTROL;
    style.visuals.widgets.hovered.bg_fill = CONTROL_HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = CONTROL_HOVER;
    style.visuals.widgets.active.bg_fill = SELECTED;
    style.visuals.widgets.active.weak_bg_fill = CONTROL_HOVER;
    style.visuals.widgets.open.bg_fill = SELECTED;
    style.visuals.widgets.open.weak_bg_fill = SELECTED;
    style.visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, FOCUS);
    style.visuals.widgets.open.fg_stroke.color = INK;
    style.visuals.widgets.inactive.fg_stroke.color = INK;
    style.visuals.widgets.hovered.fg_stroke.color = INK;
    style.visuals.widgets.active.fg_stroke.color = INK;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, FIELD_BORDER);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, FOCUS);
    style.visuals.widgets.inactive.corner_radius = 6.into();
    style.visuals.widgets.hovered.corner_radius = 6.into();
    style.visuals.widgets.active.corner_radius = 6.into();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 5.0);
    style.spacing.interact_size.y = 26.0;
    style.spacing.icon_width = 16.0;
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(21.0));
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
                visual.weak_bg_fill = fill;
                visual.bg_stroke =
                    Stroke::new(1.0_f32, if fills[0] == CONTROL { BORDER } else { fill });
                visual.corner_radius = 6.into();
                visual.expansion = 0.0;
            }
            ui.add(
                egui::Button::new((
                    egui::Atom::grow(),
                    RichText::new(label).color(ink),
                    egui::Atom::grow(),
                ))
                .min_size(size),
            )
        })
        .inner;
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            8,
            Stroke::new(2.0_f32, FOCUS),
            egui::StrokeKind::Outside,
        );
    }
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
        egui::vec2(68.0, 32.0),
        [CONTROL, CONTROL_HOVER, SOFT],
        INK,
    )
}
pub(crate) enum WalletChoice {
    Create,
    Import,
}

pub(crate) fn wallet_choice(ui: &mut Ui, width: f32, choice: WalletChoice) -> egui::Response {
    let (label, fill, ink) = match choice {
        WalletChoice::Create => ("Create wallet", ACCENT, ACCENT_INK),
        WalletChoice::Import => ("Import wallet", SELECTED, INK),
    };
    let icon = egui::Id::new(label);
    let card = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding.x = 6.0;
            ui.spacing_mut().icon_spacing = 4.0;
            egui::Button::new((
                egui::Atom::grow(),
                egui::Atom::custom(icon, egui::vec2(28.0, 28.0)),
                label,
                egui::Atom::grow(),
            ))
            .min_size(egui::vec2(width, 84.0))
            .corner_radius(10)
            .stroke(Stroke::new(1.0_f32, BORDER))
            .atom_ui(ui)
        })
        .inner;
    if let Some(rect) = card.rect(icon) {
        let painter = ui.painter();
        let center = rect.center();
        let point = |x, y| center + egui::vec2(x, y);
        let stroke = Stroke::new(1.5_f32, ink);
        painter.rect_filled(rect, 8, fill);
        if matches!(choice, WalletChoice::Create) {
            painter.line_segment([point(-5.0, 0.0), point(5.0, 0.0)], stroke);
            painter.line_segment([point(0.0, -5.0), point(0.0, 5.0)], stroke);
        } else {
            painter.line_segment([point(0.0, -7.0), point(0.0, 2.0)], stroke);
            painter.line(
                vec![point(-4.0, -2.0), point(0.0, 2.0), point(4.0, -2.0)],
                stroke,
            );
            painter.line(
                vec![point(-7.0, 2.0), point(-7.0, 6.0), point(7.0, 6.0), point(7.0, 2.0)],
                stroke,
            );
        }
    }
    focus_ring(ui, card.response.rect, card.response.has_focus());
    card.response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
pub fn danger(ui: &mut Ui, label: &str) -> egui::Response {
    button(
        ui,
        label,
        egui::vec2(100.0, 32.0),
        [DANGER, DANGER_HOVER, DANGER_PRESSED],
        INK,
    )
}
pub fn actions(ui: &mut Ui, back: &str, next: &str) -> (bool, bool) {
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(6.0);
    let mut previous = false;
    let mut proceed = false;
    ui.horizontal(|ui| {
        previous = secondary(ui, back).clicked();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            proceed = primary(ui, next).clicked();
        });
    });
    (proceed, previous)
}
pub fn backup_actions(ui: &mut Ui) -> (bool, bool, bool) {
    let (mut save, mut skip, mut cancel) = (false, false, false);
    ui.horizontal(|ui| {
        cancel = secondary(ui, "Cancel").clicked();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            save = primary(ui, "Save").clicked();
            skip = secondary(ui, "Skip & save").clicked();
        });
    });
    (save, skip, cancel)
}
pub fn recovery_actions(ui: &mut Ui, shown: bool, copied: bool) -> (bool, bool) {
    ui.horizontal(|ui| {
        ui.add_enabled_ui(shown, |ui| {
            let copy = secondary(ui, if copied { "Copied" } else { "Copy" }).clicked();
            let hide = ui
                .with_layout(Layout::right_to_left(Align::Center), |ui| {
                    secondary(ui, "Hide").clicked()
                })
                .inner;
            (copy, hide)
        })
        .inner
    })
    .inner
}

pub fn hint(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).color(MUTED));
}
pub fn label(ui: &mut Ui, text: &str) -> egui::Response {
    ui.label(RichText::new(text).size(12.0).strong())
}
pub fn page_header(ui: &mut Ui, title: &str, wallet: Option<&str>) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(RichText::new(title).heading().strong()).wrap());
        if let Some(wallet) = wallet {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                Frame::new()
                    .fill(SELECTED)
                    .corner_radius(6)
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(RichText::new(wallet).size(12.0)).truncate());
                    });
            });
        }
    })
    .response
}
pub fn input_frame() -> Frame {
    Frame::new()
        .fill(CANVAS)
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
}
pub fn toolbar_button(ui: &mut Ui, label: &str) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
            ui.visuals_mut().widgets.hovered.bg_stroke = Stroke::NONE;
            ui.visuals_mut().widgets.active.bg_stroke = Stroke::NONE;
            ui.add(
                egui::Button::new(RichText::new(label).size(18.0).color(SIDEBAR_INK))
                    .min_size(egui::vec2(28.0, 28.0))
                    .frame_when_inactive(false),
            )
        })
        .inner;
    focus_ring(ui, response.rect, response.has_focus());
    response
}
pub fn search_icon(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    paint_search_icon(ui, rect);
}
pub fn search_button(ui: &mut Ui) -> egui::Response {
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().interact_size.y = 40.0;
            toolbar_button(ui, "")
        })
        .inner;
    paint_search_icon(
        ui,
        egui::Rect::from_center_size(
            response.rect.center() + egui::vec2(2.0, 0.0),
            egui::vec2(16.0, 16.0),
        ),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Search wallets")
    });
    response.on_hover_text("Search wallets")
}
fn paint_search_icon(ui: &Ui, rect: egui::Rect) {
    let center = rect.center() - egui::vec2(2.0, 2.0);
    let stroke = Stroke::new(1.5_f32, SIDEBAR_INK);
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
    let rect = egui::Rect::from_center_size(response.rect.center(), egui::vec2(15.0, 13.0));
    let stroke = Stroke::new(1.5_f32, SIDEBAR_INK);
    let painter = ui.painter();
    if let Some(expanded) = sidebar {
        let rect = egui::Rect::from_center_size(response.rect.center(), egui::vec2(20.0, 18.0));
        let stroke = Stroke::new(1.5_f32, MUTED);
        painter.rect_stroke(rect, 4, stroke, egui::StrokeKind::Inside);
        let inset = if expanded { 0.75 } else { 4.0 };
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left() + 5.0, rect.top() + inset),
                egui::pos2(rect.left() + 6.5, rect.bottom() - inset),
            ),
            1,
            MUTED,
        );
    } else {
        let left = egui::pos2(rect.center().x - 3.0, rect.center().y);
        painter.line_segment([left, left + egui::vec2(6.0, -6.0)], stroke);
        painter.line_segment([left, left + egui::vec2(6.0, 6.0)], stroke);
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response.on_hover_text(label)
}
pub fn account_row(ui: &mut Ui, title: &str, address: Option<&str>) {
    Frame::new()
        .inner_margin(Margin::symmetric(0, 10))
        .show(ui, |ui| {
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.label(RichText::new(title).size(11.0).color(MUTED));
                ui.add(
                    egui::Label::new(
                        RichText::new(address.unwrap_or("Not synced"))
                            .monospace()
                            .size(12.0)
                            .color(INK),
                    )
                    .wrap(),
                );
            });
        });
}
pub fn recovery_mask(ui: &mut Ui, height: f32) -> egui::Response {
    let response = ui.add_sized(
        [ui.available_width(), height],
        egui::Button::new("Show seed phrase").corner_radius(RECOVERY_BODY_CORNERS),
    );
    focus_ring(ui, response.rect, response.has_focus());
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn avatar(ui: &mut Ui, name: &str) {
    let size = 28.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), size * 0.5, AVATAR);
    let letter = name
        .chars()
        .next()
        .unwrap_or('K')
        .to_ascii_uppercase()
        .to_string();
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        FontId::proportional(size * 0.55),
        INK,
    );
}
pub fn wallet_row(ui: &mut Ui, name: &str, selected: bool, compact: bool) -> egui::Response {
    let width = ui.available_width();
    let mut frame = Frame::new()
        .corner_radius(6)
        .inner_margin(Margin::symmetric(8, 4))
        .begin(ui);
    {
        let ui = &mut frame.content_ui;
        ui.set_width((width - 16.0).max(if compact { 28.0 } else { 80.0 }));
        ui.horizontal(|ui| {
            ui.set_height(32.0);
            avatar(ui, name);
            if !compact {
                ui.add(egui::Label::new(RichText::new(name)).truncate());
            }
        });
    }
    let row = ui.allocate_rect(
        frame.frame.outer_rect(frame.content_ui.min_rect()),
        egui::Sense::click(),
    );
    frame.frame.fill = if selected {
        SELECTED
    } else if row.hovered() {
        CONTROL_HOVER
    } else {
        TRANSPARENT
    };
    frame.paint(ui);
    focus_ring(ui, row.rect, row.has_focus());
    row.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            name,
        )
    });
    let row = row.on_hover_cursor(egui::CursorIcon::PointingHand);
    if compact {
        row.on_hover_text(name)
    } else {
        row
    }
}
pub fn focus_ring(ui: &Ui, rect: egui::Rect, focused: bool) {
    if focused {
        ui.painter().rect_stroke(
            rect,
            6,
            Stroke::new(2.0_f32, FOCUS),
            egui::StrokeKind::Inside,
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
                        let width = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).floor();
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
    fn dusk_palette_keeps_text_and_focus_legible() {
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
            CANVAS,
            SURFACE,
            CONTENT,
            SOFT,
            SELECTED,
            CONTROL,
            CONTROL_HOVER,
            WARNING_SOFT,
        ] {
            assert!(contrast(INK, surface) >= 4.5);
            assert!(contrast(MUTED, surface) >= 4.5);
            assert!(contrast(FOCUS, surface) >= 3.0);
        }
        for fill in [ACCENT, ACCENT_HOVER, ACCENT_PRESSED] {
            assert!(contrast(ACCENT_INK, fill) >= 4.5);
        }
        for fill in [DANGER, DANGER_HOVER, DANGER_PRESSED] {
            assert!(contrast(INK, fill) >= 4.5);
        }
        for surface in [CANVAS, SEARCH] {
            assert!(contrast(FIELD_BORDER, surface) >= 3.0);
        }
        assert!(contrast(WARNING_INK, WARNING_SOFT) >= 4.5);
        assert!(contrast(DANGER_INK, SURFACE) >= 4.5);
        assert!(contrast(FOCUS, SURFACE) >= 4.5);
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
    fn recovery_mask_fits_the_detail_panel() {
        for width in [280.0, 404.0] {
            let ctx = egui::Context::default();
            configure(&ctx);
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width + 32.0, 200.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(Frame::new().inner_margin(16))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            let mask = recovery_mask(ui, 124.0);
                            assert!(bounds.contains_rect(mask.rect));
                            assert_eq!(mask.rect.width(), width);
                        });
                },
            );
        }
    }

    #[test]
    fn account_rows_wrap_public_addresses_within_the_panel() {
        for width in [224.0, 348.0, 364.0, 520.0] {
            let ctx = egui::Context::default();
            configure(&ctx);
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width + 32.0, 360.0),
                )),
                ..Default::default()
            };
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default()
                    .frame(Frame::new().inner_margin(16))
                    .show(ctx, |ui| {
                        let bounds = ui.max_rect();
                        for address in [
                            Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"),
                            Some("B9sVeu4rJU12oUrUtzjc6BSNuEXdfvurZkdcaTVkP2LY"),
                            None,
                        ] {
                            let row = ui.scope(|ui| account_row(ui, "Ethereum", address));
                            assert!(bounds.contains_rect(row.response.rect));
                            if width >= 348.0 {
                                assert!(row.response.rect.height() <= 55.0);
                            }
                        }
                    });
            });
        }
    }
    #[test]
    fn wallet_rows_fit_sidebar_and_narrow_window() {
        for width in [44.0, 124.0, 140.0, 164.0, 192.0, 236.0, 364.0] {
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
                        let row = wallet_row(
                            ui,
                            &"long-wallet-".repeat(5),
                            true,
                            width == 44.0,
                        );
                        assert!(bounds.contains_rect(row.rect));
                        assert!(row.rect.width() <= width + 1.0);
                        assert!((row.rect.height() - 40.0).abs() < 0.5);
                    });
            });
        }
    }
    #[test]
    fn wallet_row_can_be_activated_with_the_keyboard() {
        for compact in [false, true] {
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
                        let row = wallet_row(ui, "Public fixture", false, compact);
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
