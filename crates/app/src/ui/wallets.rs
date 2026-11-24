use super::{MIN_RECOVERY_CARD_HEIGHT, Stage, WalletApp, phrase_card};
use crate::{
    model::{DEFAULT_ETH_PATH, DEFAULT_SOL_PATH, WalletSummary},
    theme::{self, RowValue},
};
use eframe::egui::{self, Align, Frame, Id, Layout, Margin, RichText, ViewportCommand};
use std::time::Instant;

const FOCUS_SEARCH: &str = "focus-wallet-search";

fn wallet_search(ui: &mut egui::Ui, search: &mut String) -> egui::Response {
    let background = ui.interact(
        egui::Rect::from_min_size(
            ui.available_rect_before_wrap().min,
            egui::vec2(ui.available_width(), 30.0),
        ),
        Id::new("wallet-search-background"),
        egui::Sense::CLICK,
    );
    let field = theme::input_frame()
        .fill(theme::SEARCH)
        // The magnifier identifies the field, so it can use the quieter card hairline.
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(7)
        .inner_margin(Margin {
            left: 9,
            ..Margin::symmetric(8, 6)
        })
        .show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 16.0;
            ui.spacing_mut().item_spacing.x = 5.0;
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
            })
            .inner
        });
    let focus_search = ui
        .data_mut(|data| data.remove_temp::<bool>(Id::new(FOCUS_SEARCH)))
        .unwrap_or(false);
    if background.clicked() || focus_search {
        field.inner.request_focus();
    }
    theme::focus_ring(ui, field.response.rect, field.inner.has_focus());
    field.inner
}

/// Lets the transparent title bar area drag the window, since content fills it.
fn window_drag_area(ui: &mut egui::Ui, rect: egui::Rect, id: &str) {
    let response = ui.interact(rect, Id::new(id), egui::Sense::click_and_drag());
    if response.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if response.double_clicked() {
        let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }
}

fn toolbar_row(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    let rect = egui::Rect::from_min_size(
        ui.available_rect_before_wrap().min,
        egui::vec2(ui.available_width(), theme::TOOLBAR_HEIGHT),
    );
    window_drag_area(ui, rect, id);
    ui.allocate_ui_with_layout(rect.size(), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(rect.size());
        ui.spacing_mut().item_spacing.x = 4.0;
        add(ui)
    });
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

    fn wallet_rows(&mut self, ui: &mut egui::Ui) {
        let selected = if self.stage == Stage::Wallets {
            self.selected_entry().map(|entry| entry.name.clone())
        } else {
            self.selected_wallet
                .clone()
                .or_else(|| self.selected_entry().map(|entry| entry.name.clone()))
        };
        let mut clicked = None;
        for entry in self
            .wallets
            .iter()
            .filter(|entry| entry.matches(&self.search))
        {
            ui.push_id(&entry.name, |ui| {
                let subtitle = entry
                    .ethereum_address
                    .as_deref()
                    .map_or_else(|| "Not synced".to_owned(), theme::abbreviate_address);
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
            self.select_wallet(name);
        }
    }

    fn add_wallet(&mut self) {
        self.wipe_draft();
        self.message.clear();
        self.stage = Stage::Welcome;
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        ui.add_space((ui.available_height() * 0.22).clamp(24.0, 120.0));
        ui.vertical_centered(|ui| {
            let empty = self.wallets.is_empty();
            ui.label(
                RichText::new(if empty { "No wallets" } else { "No results" })
                    .font(theme::medium_font(20.0)),
            );
            ui.add_space(2.0);
            let detail = if empty {
                "Create a new wallet or import an existing seed phrase.".to_owned()
            } else {
                format!("No wallets match \u{201c}{}\u{201d}.", self.search.trim())
            };
            ui.add(egui::Label::new(RichText::new(detail).color(theme::MUTED)).wrap());
            if empty {
                ui.add_space(14.0);
                if theme::primary(ui, "Add wallet").clicked() {
                    self.add_wallet();
                }
            }
        });
    }

    pub(super) fn wallet_detail(&mut self, ui: &mut egui::Ui) {
        let Some(entry) = self.selected_entry().cloned() else {
            self.account_view = super::accounts::AccountView::default();
            self.empty_state(ui);
            return;
        };
        if self.name != entry.name {
            self.revealed.clear();
        }
        self.account_view.select_wallet(&entry.name);
        self.account_view.set_cached(&entry);
        theme::detail_header(ui, &entry.name, "Ethereum and Solana");
        ui.add_space(14.0);
        let copied = self.copied.filter(|_| self.clipboard.has_copy());
        let mut copy = None;
        let mut show_phrase = false;
        theme::group(ui, |ui| {
            for (label, address) in [
                ("Ethereum", entry.ethereum_address.as_deref()),
                ("Solana", entry.solana_address.as_deref()),
            ] {
                let row = match address {
                    // Abbreviated until clicked; the click copies and shows it in full
                    // for as long as the copy stays on the clipboard.
                    Some(address) if copied == Some(label) => {
                        theme::field_row(ui, label, RowValue::Mono(address), Some("Copied"))
                    }
                    Some(address) => theme::field_row(
                        ui,
                        label,
                        RowValue::Mono(&theme::abbreviate_address(address)),
                        Some("Copy"),
                    ),
                    None => theme::field_row(ui, label, RowValue::Muted("Not synced"), None),
                };
                if let Some(address) = address.filter(|_| row.clicked()) {
                    copy = Some((label, address.to_owned()));
                }
                theme::group_separator(ui);
            }
            show_phrase = theme::field_row(
                ui,
                "Seed phrase",
                RowValue::Text("••••••••••••"),
                Some("Show"),
            )
            .clicked();
        });
        theme::footnote(
            ui,
            &format!("Cached from {DEFAULT_ETH_PATH} and {DEFAULT_SOL_PATH}."),
        );
        if let Some((label, address)) = copy {
            match self.clipboard.copy(&address) {
                Ok(()) => {
                    self.copied = Some(label);
                    self.message.clear();
                }
                Err(_) => self.message = "Unable to copy the address.".into(),
            }
        }
        if show_phrase {
            self.wipe_draft();
            self.name = entry.name.clone();
            self.selected_wallet = Some(entry.name.clone());
            self.recovery_card_height = MIN_RECOVERY_CARD_HEIGHT;
            self.message.clear();
            self.stage = Stage::ViewPhrase;
            return;
        }
        theme::section(ui, "Derived accounts");
        self.derived_account(ui);
        ui.add_space(14.0);
        let delete = theme::group(ui, |ui| theme::destructive_row(ui, "Delete wallet…")).inner;
        if delete.clicked() {
            self.confirm_delete(vec![entry.name.clone()]);
        }
        ui.add_space(12.0);
    }

    /// Draws the phrase card and reports a click on the masked placeholders.
    pub(super) fn recovery_card(&mut self, ui: &mut egui::Ui) -> bool {
        let focused = ui.ctx().input(|input| input.focused);
        let phrase = self.revealed.visible(Instant::now(), focused);
        let shown = phrase.is_some();
        let card = phrase_card(ui, phrase, self.recovery_card_height);
        if shown {
            self.recovery_card_height = card.rect.height();
        }
        !shown && card.clicked()
    }

    fn open_settings(&mut self) {
        self.wipe_draft();
        self.message.clear();
        let mut minutes = |ttl: crate::model::Result<u64>| match ttl {
            Ok(seconds) => (seconds / 60).to_string(),
            Err(_) => {
                self.message = "Unable to read settings. Enter a duration to save.".into();
                String::new()
            }
        };
        let node = minutes(self.vault.authentication_ttl());
        let signer = minutes(self.vault.signer_authentication_ttl());
        self.authentication_minutes = node;
        self.signer_authentication_minutes = signer;
        self.stage = Stage::Settings;
    }

    fn wallet_controls(&mut self, ui: &mut egui::Ui) {
        let more = theme::more_button(ui);
        theme::focus_ring(ui, more.rect, more.has_focus());
        egui::Popup::menu(&more).show(|ui| {
            ui.set_min_width(180.0);
            if ui.button("Settings…").clicked() {
                self.open_settings();
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
                    egui::Button::new(
                        RichText::new("Delete all wallets…").color(theme::DANGER_INK),
                    ),
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
        if theme::add_button(ui).clicked() {
            self.add_wallet();
        }
    }

    fn sidebar_toggle(&mut self, ui: &mut egui::Ui) {
        let label = if self.sidebar_visible {
            "Hide sidebar"
        } else {
            "Show sidebar"
        };
        if theme::navigation_button(ui, label, Some(self.sidebar_visible)).clicked() {
            self.sidebar_visible = !self.sidebar_visible;
        }
    }

    fn content_toolbar(&mut self, ui: &mut egui::Ui) {
        toolbar_row(ui, "content-toolbar", |ui| {
            if self.sidebar_visible {
                ui.add_space(10.0);
            } else {
                ui.add_space(theme::TRAFFIC_LIGHTS);
                self.sidebar_toggle(ui);
                if theme::search_button(ui).clicked() {
                    self.sidebar_visible = true;
                    ui.data_mut(|data| data.insert_temp(Id::new(FOCUS_SEARCH), true));
                }
            }
            if self.stage.previous().is_some()
                && theme::navigation_button(ui, "Back", None).clicked()
            {
                self.navigate_back();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(10.0);
                if self.stage == Stage::Wallets {
                    self.wallet_controls(ui);
                }
            });
        });
    }

    fn wallet_sidebar(&mut self, ui: &mut egui::Ui) {
        toolbar_row(ui, "sidebar-toolbar", |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                self.sidebar_toggle(ui);
            });
        });
        ui.add_space(10.0);
        if wallet_search(ui, &mut self.search).changed() {
            self.revealed.clear();
            self.clipboard.clear();
        }
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Wallets")
                    .font(theme::medium_font(11.0))
                    .color(theme::MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(self.wallets.len().to_string())
                        .size(11.0)
                        .color(theme::MUTED),
                );
            });
        });
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .id_salt("wallet-list")
            .auto_shrink([false, false])
            .show(ui, |ui| self.wallet_rows(ui));
    }

    pub(super) fn wallet_browser(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, focused: bool) {
        if ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::F,
            ))
        }) {
            self.sidebar_visible = true;
            ctx.data_mut(|data| data.insert_temp(Id::new(FOCUS_SEARCH), true));
        }
        if self.sidebar_visible {
            egui::SidePanel::left("wallet-sidebar")
                .show_separator_line(true)
                .default_width(200.0)
                .width_range(180.0..=(ui.available_width() - 380.0).clamp(180.0, 300.0))
                .resizable(true)
                .frame(Frame::new().fill(theme::SIDEBAR).inner_margin(Margin {
                    left: 10,
                    right: 10,
                    top: 0,
                    bottom: 10,
                }))
                .show_inside(ui, |ui| {
                    if self.stage == Stage::Fatal {
                        ui.disable();
                    }
                    self.wallet_sidebar(ui);
                });
        }
        egui::CentralPanel::default()
            .frame(Frame::new().fill(theme::CONTENT))
            .show_inside(ui, |ui| {
                self.content_toolbar(ui);
                Frame::new()
                    .inner_margin(theme::PAGE_MARGIN)
                    .show(ui, |ui| self.page_content(ui, ctx, focused));
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
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 160.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        field_rect = ui.scope(|ui| wallet_search(ui, &mut search)).response.rect;
                    });
                },
            );
            let pos = egui::pos2(field_rect.left() + 4.0, field_rect.center().y);
            let mut raw = egui::RawInput::default();
            raw.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                raw.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
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
            let icon_x = |output: &egui::FullOutput| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Circle(circle) if circle.radius == 4.5 => {
                            Some(circle.center.x)
                        }
                        _ => None,
                    })
                    .unwrap()
            };
            assert!((icon_x(&output) - icon_x(&focused)).abs() < 0.5);
            let typed = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Text("wallet".into())],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        wallet_search(ui, &mut search);
                    });
                },
            );
            assert!((icon_x(&output) - icon_x(&typed)).abs() < 0.5);
            assert_eq!(search, "wallet");
        }
    }

    #[test]
    fn sidebar_rows_stack_below_search_within_the_sidebar() {
        for width in [180.0, 300.0] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut search = String::new();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(Frame::new().inner_margin(Margin::symmetric(10, 12)))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            let field = ui.scope(|ui| wallet_search(ui, &mut search)).response.rect;
                            assert!(bounds.contains_rect(field));
                            assert!(field.height() <= 32.0, "{field:?}");
                            let mut previous = field.bottom();
                            for name in ["First wallet", "Second wallet", "Third wallet"] {
                                let row = theme::wallet_row(ui, name, "0xf39F…2266", false).rect;
                                ui.add_space(2.0);
                                assert!(bounds.contains_rect(row));
                                assert!(row.top() >= previous);
                                previous = row.bottom();
                            }
                        });
                },
            );
        }
    }

    #[test]
    fn expanding_search_focuses_only_a_rendered_accessibility_node() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut search = String::new();
        for expanded in [false, true] {
            let output = ctx.run(
                egui::RawInput {
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if expanded {
                            assert!(wallet_search(ui, &mut search).has_focus());
                        } else {
                            theme::search_button(ui);
                            ui.data_mut(|data| {
                                data.insert_temp(Id::new("focus-wallet-search"), true)
                            });
                        }
                    });
                },
            );
            let tree = output.platform_output.accesskit_update.unwrap();
            assert!(tree.nodes.iter().any(|(id, _)| *id == tree.focus));
            if expanded {
                assert_eq!(
                    ctx.memory(|memory| memory.focused()),
                    Some(Id::new("wallet-search"))
                );
            }
        }
    }
}
