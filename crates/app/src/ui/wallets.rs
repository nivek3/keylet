use super::{revealed_card, Stage, WalletApp};
use crate::{
    catalog::Catalog,
    model::{WalletSummary, DEFAULT_ETH_PATH, DEFAULT_SOL_PATH},
    theme,
};
use eframe::egui::{self, Align, Color32, Frame, Id, Layout, Margin, RichText};
use std::time::Instant;

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

    fn wallet_rows(&mut self, ui: &mut egui::Ui) {
        let selected = self.selected_entry().map(|entry| entry.name.clone());
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
                    if value.len() > 18 {
                        format!("{}…{}", &value[..8], &value[value.len() - 6..])
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
                )
                .clicked()
                {
                    clicked = Some(entry.name.clone());
                }
            });
            ui.add_space(2.0);
        }
        if let Some(name) = clicked {
            self.revealed.clear();
            self.selected_wallet = Some(name);
            self.show_wallet_details = true;
        }
        if selected.is_none() {
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

    fn wallet_detail(&mut self, ui: &mut egui::Ui) {
        let Some(entry) = self.selected_entry().cloned() else {
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
        let width = ui.available_width().min(380.0);
        ui.vertical_centered(|ui| {
            ui.set_width(width);
            theme::wallet_heading(ui, &entry.name);
            ui.add_space(16.0);
            Frame::new()
                .fill(theme::CANVAS)
                .corner_radius(9)
                .inner_margin(Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    ui.set_width((width - 28.0).max(100.0));
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        theme::account_row(ui, "Ethereum", entry.ethereum_address.as_deref());
                        ui.separator();
                        theme::account_row(ui, "Solana", entry.solana_address.as_deref());
                        ui.separator();
                        ui.add_space(4.0);
                        ui.collapsing("Derivation paths", |ui| {
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
            ui.horizontal(|ui| {
                if theme::secondary(ui, "Seed phrase").clicked() {
                    self.wipe_draft();
                    self.name = entry.name.clone();
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
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                    ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                    ui.visuals_mut().widgets.hovered.bg_stroke = egui::Stroke::NONE;
                    ui.visuals_mut().widgets.active.bg_stroke = egui::Stroke::NONE;
                    ui.menu_button("•••", |ui| {
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
                    });
                });
                if theme::toolbar_button(ui, "+")
                    .on_hover_text("Add wallet")
                    .clicked()
                {
                    self.wipe_draft();
                    self.message.clear();
                    self.stage = Stage::Welcome;
                }
            });
    }

    fn wallet_sidebar(&mut self, ui: &mut egui::Ui) {
        let search = theme::input_frame()
            .fill(theme::SEARCH)
            .stroke(egui::Stroke::NONE)
            .corner_radius(6)
            .inner_margin(Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.spacing_mut().interact_size.y = 18.0;
                ui.horizontal(|ui| {
                    theme::search_icon(ui);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .id(Id::new("wallet-search"))
                            .hint_text("Search")
                            .char_limit(128)
                            .frame(false)
                            .margin(Margin::ZERO)
                            .desired_width(f32::INFINITY),
                    )
                })
                .inner
            });
        theme::focus_ring(ui, search.response.rect, search.inner.has_focus());
        if search.inner.changed() {
            self.revealed.clear();
            self.show_wallet_details = false;
        }
        ui.add_space(8.0);
        if !self.message.is_empty() {
            ui.colored_label(theme::DANGER, &self.message);
            ui.add_space(8.0);
        }
        egui::ScrollArea::vertical()
            .id_salt("wallet-list")
            .auto_shrink([false, false])
            .show(ui, |ui| self.wallet_rows(ui));
    }

    pub(super) fn wallet_browser(&mut self, ui: &mut egui::Ui) {
        if ui.available_width() >= 632.0 || !self.sidebar_visible {
            if self.sidebar_visible {
                egui::SidePanel::left("wallet-sidebar")
                .exact_width(196.0)
                .resizable(false)
                .frame(
                    Frame::new()
                        .fill(theme::CANVAS)
                        .inner_margin(Margin::symmetric(12, 12)),
                )
                    .show_inside(ui, |ui| self.wallet_sidebar(ui));
            }
            egui::CentralPanel::default()
                .frame(
                    Frame::new()
                        .fill(Color32::WHITE)
                        .inner_margin(Margin::symmetric(24, 24)),
                )
                .show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("wallet-detail")
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.wallet_detail(ui));
                });
        } else {
            Frame::new()
                .fill(if self.show_wallet_details {
                    Color32::WHITE
                } else {
                    theme::CANVAS
                })
                .inner_margin(20)
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    if self.show_wallet_details {
                        egui::ScrollArea::vertical()
                            .id_salt("wallet-detail-narrow")
                            .show(ui, |ui| self.wallet_detail(ui));
                    } else {
                        self.wallet_sidebar(ui);
                    }
                });
        }
    }
}
