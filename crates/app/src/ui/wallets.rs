use super::{revealed_card, Stage, WalletApp};
use crate::{
    catalog::Catalog,
    model::{WalletSummary, DEFAULT_ETH_PATH, DEFAULT_SOL_PATH},
    theme,
};
use eframe::egui::{self, Align, Frame, Id, Layout, Margin, RichText};
use std::time::Instant;

fn wallet_search(ui: &mut egui::Ui, search: &mut String) -> egui::Response {
    let background = ui.interact(
        egui::Rect::from_min_size(ui.available_rect_before_wrap().min, egui::vec2(ui.available_width(), 40.0)),
        Id::new("wallet-search-background"), egui::Sense::CLICK,
    );
    let field = theme::input_frame()
        .fill(theme::SEARCH)
        .corner_radius(6)
        .inner_margin(Margin { left: 15, ..Margin::symmetric(8, 10) })
        .show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 18.0;
            ui.horizontal(|ui| {
                theme::search_icon(ui);
                ui.add(
                    egui::TextEdit::singleline(search)
                        .id(Id::new("wallet-search"))
                        .hint_text("Search")
                        .char_limit(128)
                        .frame(false)
                        .margin(Margin::ZERO)
                        .desired_width(f32::INFINITY),
                )
            }).inner
        });
    let focus_search = ui.data_mut(|data| data.remove_temp::<bool>(Id::new("focus-wallet-search"))).unwrap_or(false);
    if background.clicked() || focus_search {
        field.inner.request_focus();
    }
    theme::focus_ring(ui, field.response.rect, field.inner.has_focus());
    field.inner
}

impl WalletApp {
    fn selected_entry(&self) -> Option<&WalletSummary> {
        let visible = || {
            self.wallets
                .iter()
                .filter(|entry| entry.matches(&self.search))
        };
        visible()
            .find(|entry| Some(&entry.name) == self.selected_wallet.as_ref())
            .or_else(|| visible().next())
    }

    fn wallet_rows(&mut self, ui: &mut egui::Ui, compact: bool) {
        let selected = if self.stage == Stage::Wallets {
            self.selected_entry().map(|entry| entry.name.clone())
        } else {
            self.selected_wallet.clone().or_else(|| self.selected_entry().map(|entry| entry.name.clone()))
        };
        let mut clicked = None;
        for entry in self
            .wallets
            .iter()
            .filter(|entry| entry.matches(&self.search))
        {
            let address = entry
                .ethereum_address
                .as_ref()
                .or(entry.solana_address.as_ref());
            let subtitle = address
                .map(|value| {
                    if value.len() > 8 {
                        format!("{}…{}", &value[..4], &value[value.len() - 4..])
                    } else {
                        value.clone()
                    }
                })
                .unwrap_or_else(|| "Not synced".into());
            ui.push_id(&entry.name, |ui| {
                if theme::wallet_row(
                    ui,
                    &entry.name,
                    &subtitle,
                    selected.as_ref() == Some(&entry.name),
                    compact,
                )
                .clicked()
                {
                    clicked = Some(entry.name.clone());
                }
            });
            ui.add_space(2.0);
        }
        if let Some(name) = clicked {
            self.select_wallet(name);
        }
        if selected.is_none() && !compact {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                theme::hint(
                    ui,
                    if self.wallets.is_empty() {
                        "No wallets yet"
                    } else {
                        "No results"
                    },
                );
            });
        }
    }

    pub(super) fn wallet_detail(&mut self, ui: &mut egui::Ui) {
        let Some(entry) = self.selected_entry().cloned() else {
            self.account_view = super::accounts::AccountView::default();
            ui.add_space(100.0);
            ui.vertical_centered(|ui| {
                theme::app_icon(ui);
                ui.add_space(16.0);
                ui.label(
                    RichText::new(if self.wallets.is_empty() { "Your wallets, kept private" } else { "No matching wallets" })
                        .size(17.0)
                        .strong(),
                );
                ui.add_space(6.0);
                theme::hint(ui, if self.wallets.is_empty() {
                    "Use + to create or import a wallet."
                } else {
                    "Try another wallet name or address."
                });
            });
            return;
        };
        if self.name != entry.name {
            self.revealed.clear();
        }
        self.account_view.select_wallet(&entry.name);
        let width = ui.available_width().min(380.0);
        ui.vertical_centered(|ui| {
            ui.set_width(width);
            theme::wallet_heading(ui, &entry.name);
            ui.add_space(24.0);
            Frame::new()
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        theme::account_row(ui, "Ethereum", entry.ethereum_address.as_deref());
                        ui.separator();
                        theme::account_row(ui, "Solana", entry.solana_address.as_deref());
                        ui.separator();
                        ui.add_space(4.0);
                        ui.collapsing(RichText::new("Derivation paths").size(12.0).color(theme::MUTED), |ui| {
                            theme::label(ui, "Ethereum");
                            ui.label(
                                RichText::new(DEFAULT_ETH_PATH)
                                    .monospace()
                                    .color(theme::MUTED),
                            );
                            ui.add_space(4.0);
                            theme::label(ui, "Solana");
                            ui.label(
                                RichText::new(DEFAULT_SOL_PATH)
                                    .monospace()
                                    .color(theme::MUTED),
                            );
                        });
                    });
                });
            ui.add_space(12.0);
            ui.collapsing("Derived account", |ui| self.derived_account(ui));
            ui.add_space(20.0);
            ui.horizontal(|ui| {
                if theme::secondary(ui, "Seed phrase").clicked() {
                    self.wipe_draft();
                    self.name = entry.name.clone();
                    self.selected_wallet = Some(entry.name.clone());
                    self.recovery_card_height = 168.0;
                    self.message.clear();
                    self.stage = Stage::ViewPhrase;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if theme::secondary(ui, "Delete").clicked() {
                        self.confirm_delete(vec![entry.name.clone()]);
                    }
                });
            });
        });
    }

    pub(super) fn recovery_card(&mut self, ui: &mut egui::Ui, name: &str) {
        let focused = ui.ctx().input(|input| input.focused);
        if let Some(phrase) = self.revealed.visible(Instant::now(), focused) {
            let card = revealed_card(ui, phrase, self.recovery_card_height);
            self.recovery_card_height = card.rect.height();
        } else if theme::recovery_mask(ui, self.recovery_card_height).clicked() && focused {
            if let Err(error) = self.reveal_wallet(name) {
                self.message = format!("Seed phrase not shown ({}).", error.code());
            }
        }
    }

    pub(super) fn wallet_controls(&mut self, ui: &mut egui::Ui) {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.scope(|ui| {
                    ui.spacing_mut().interact_size = egui::vec2(28.0, 28.0);
                    ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = theme::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                    ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::NONE;
                    ui.visuals_mut().widgets.active.bg_stroke = egui::Stroke::NONE;
                    let more = ui.menu_button("•••", |ui| {
                        if ui.button("Settings…").clicked() {
                            self.wipe_draft();
                            self.message.clear();
                            self.authentication_minutes = match Catalog::for_group(&self.vault.group)
                                .and_then(|catalog| catalog.authentication_ttl()) {
                                    Ok(seconds) => (seconds / 60).to_string(),
                                    Err(_) => {
                                        self.message = "Unable to read settings. Enter a duration to save.".into();
                                        String::new()
                                    }
                                };
                            self.stage = Stage::Settings;
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Refresh").clicked() {
                            self.show_wallets();
                            ui.close();
                        }
                        if ui.button("Sync from Keychain").clicked() {
                            self.sync_wallets();
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                !self.wallets.is_empty(),
                                egui::Button::new("Delete all wallets…"),
                            )
                            .clicked()
                        {
                            self.confirm_delete(
                                self.wallets
                                    .iter()
                                    .map(|entry| entry.name.clone())
                                    .collect(),
                            );
                            ui.close();
                        }
                    }).response;
                    more.widget_info(|| egui::WidgetInfo::labeled(
                        egui::WidgetType::Button, ui.is_enabled(), "More actions",
                    ));
                    theme::focus_ring(ui, more.rect, more.has_focus());
                    more.on_hover_text("More actions");
                });
                let add = theme::toolbar_button(ui, "+");
                add.widget_info(|| egui::WidgetInfo::labeled(
                    egui::WidgetType::Button, ui.is_enabled(), "Add wallet",
                ));
                if add.on_hover_text("Add wallet").clicked() {
                    self.wipe_draft();
                    self.message.clear();
                    self.stage = Stage::Welcome;
                }
            });
    }

    fn wallet_sidebar(&mut self, ui: &mut egui::Ui) {
        if wallet_search(ui, &mut self.search).changed() {
            self.revealed.clear();
            self.clipboard.clear();
        }
        ui.add_space(8.0);
        if self.stage == Stage::Wallets && !self.message.is_empty() {
            ui.colored_label(theme::DANGER_INK, &self.message);
            ui.add_space(8.0);
        }
        egui::ScrollArea::vertical()
            .id_salt("wallet-list")
            .auto_shrink([false, false])
            .show(ui, |ui| self.wallet_rows(ui, false));
    }

    pub(super) fn wallet_browser(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, focused: bool) {
        let expanded = self.sidebar_visible;
        egui::SidePanel::left(if expanded { "wallet-sidebar" } else { "wallet-rail" })
            .show_separator_line(true)
            .default_width(if expanded { 144.0 } else { 56.0 })
            .width_range(if expanded {
                144.0..=(ui.available_width() - 320.0).clamp(144.0, 320.0)
            } else { 56.0..=56.0 })
            .resizable(expanded)
            .frame(
                Frame::new()
                    .fill(if self.vibrant_shell { theme::TRANSPARENT } else { theme::CANVAS })
                    .inner_margin(Margin::symmetric(if expanded { 10 } else { 6 }, 12)),
            )
            .show_inside(ui, |ui| {
                if self.stage == Stage::Fatal {
                    ui.disable();
                }
                if expanded {
                    self.wallet_sidebar(ui);
                } else {
                    ui.vertical_centered(|ui| {
                        if theme::search_button(ui).clicked() {
                            self.sidebar_visible = true;
                            ui.data_mut(|data| data.insert_temp(Id::new("focus-wallet-search"), true));
                        }
                    });
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical()
                        .id_salt("wallet-rail-list")
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.wallet_rows(ui, true));
                }
            });
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme::CONTENT)
                    .inner_margin(Margin::symmetric(24, 24)),
            )
            .show_inside(ui, |ui| {
                if self.stage == Stage::Wallets && !expanded && !self.message.is_empty() {
                    ui.colored_label(theme::DANGER_INK, &self.message);
                    ui.add_space(8.0);
                }
                self.page_content(ui, ctx, focused);
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_keeps_its_position_and_the_whole_field_clickable() {
        for width in [144.0, 320.0] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut search = String::new();
            let mut field_rect = egui::Rect::NOTHING;
            let output = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 160.0))),
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    field_rect = ui.scope(|ui| wallet_search(ui, &mut search)).response.rect;
                });
            });
            let pos = egui::pos2(field_rect.left() + 4.0, field_rect.center().y);
            let mut raw = egui::RawInput::default();
            raw.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                raw.events.push(egui::Event::PointerButton {
                    pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE,
                });
            }
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(wallet_search(ui, &mut search).has_focus());
                });
            });
            let focused = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(wallet_search(ui, &mut search).has_focus());
                });
            });
            let icon_x = |output: &egui::FullOutput| output.shapes.iter().find_map(|shape| {
                match &shape.shape {
                    egui::Shape::Circle(circle) if circle.radius == 4.5 => Some(circle.center.x),
                    _ => None,
                }
            }).unwrap();
            assert!((icon_x(&output) - icon_x(&focused)).abs() < 0.5);
            let typed = ctx.run(egui::RawInput {
                events: vec![egui::Event::Text("wallet".into())],
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { wallet_search(ui, &mut search); });
            });
            assert!((icon_x(&output) - icon_x(&typed)).abs() < 0.5);
            assert_eq!(search, "wallet");
        }
    }

    #[test]
    fn sidebar_rows_keep_their_vertical_positions_when_collapsed() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut search = String::new();
        let mut expanded_rows: Option<Vec<egui::Rect>> = None;
        for expanded in [true, false, true] {
            let mut rows = Vec::new();
            let mut search_rect = egui::Rect::NOTHING;
            let output = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(if expanded { 144.0 } else { 56.0 }, 400.0),
                )),
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default()
                    .frame(Frame::new().inner_margin(Margin::symmetric(if expanded { 10 } else { 6 }, 12)))
                    .show(ctx, |ui| {
                        search_rect = if expanded {
                            ui.scope(|ui| wallet_search(ui, &mut search)).response.rect
                        } else {
                            ui.vertical_centered(|ui| theme::search_button(ui)).response.rect
                        };
                        ui.add_space(8.0);
                        for name in ["First wallet", "Second wallet", "Third wallet"] {
                            rows.push(theme::wallet_row(ui, name, "0x1234…abcd", false, !expanded).rect);
                            ui.add_space(2.0);
                        }
                    });
            });
            let circle_x = |radius| output.shapes.iter().find_map(|shape| {
                match &shape.shape {
                    egui::Shape::Circle(circle) if circle.radius == radius => Some(circle.center.x),
                    _ => None,
                }
            }).unwrap();
            assert!((circle_x(4.5) - circle_x(14.0)).abs() < 0.5);
            assert!((search_rect.height() - 40.0).abs() < 0.5);
            for row in &rows {
                assert!((row.height() - search_rect.height()).abs() < 0.5);
            }
            if let Some(expected) = &expanded_rows {
                for (actual, expected) in rows.iter().zip(expected) {
                    assert!((actual.top() - expected.top()).abs() < 0.5, "expanded={expanded}, actual={actual:?}, expected={expected:?}");
                    assert!((actual.height() - expected.height()).abs() < 0.5);
                }
            } else {
                expanded_rows = Some(rows);
            }
        }
    }

    #[test]
    fn expanding_search_focuses_only_a_rendered_accessibility_node() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut search = String::new();
        for expanded in [false, true] {
            let output = ctx.run(egui::RawInput { focused: true, ..Default::default() }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if expanded {
                        assert!(wallet_search(ui, &mut search).has_focus());
                    } else {
                        theme::search_button(ui);
                        ui.data_mut(|data| data.insert_temp(Id::new("focus-wallet-search"), true));
                    }
                });
            });
            let tree = output.platform_output.accesskit_update.unwrap();
            assert!(tree.nodes.iter().any(|(id, _)| *id == tree.focus));
            if expanded {
                assert_eq!(ctx.memory(|memory| memory.focused()), Some(Id::new("wallet-search")));
            }
        }
    }
}
