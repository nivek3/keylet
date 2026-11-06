mod accounts;
mod wallets;

use crate::catalog::Catalog;
use crate::theme;
use crate::{
    macos::{self, Clipboard, ClipboardGate, SecureInput, Vault},
    model::*,
};
use eframe::egui::{self, Event, Id, TextEdit, ViewportCommand};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

const SEED_CLIPBOARD_NOTICE: &str =
    "Clears after 60s while open. Other apps may keep a copy.";
const MIN_RECOVERY_CARD_HEIGHT: f32 = 184.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Stage {
    Welcome,
    Wallets,
    Settings,
    Details,
    Secrets,
    Reveal,
    Backup,
    Confirm,
    Success,
    ViewPhrase,
    Delete,
    Fatal,
}
impl Stage {
    fn blocks_clipboard(self) -> bool {
        matches!(self, Self::Secrets | Self::Reveal | Self::Backup | Self::Confirm | Self::ViewPhrase)
    }
    fn has_draft(self) -> bool {
        matches!(self, Self::Details | Self::Secrets | Self::Reveal | Self::Backup | Self::Confirm)
    }
    fn previous(self) -> Option<Self> {
        match self {
            Self::Wallets | Self::Fatal => None,
            Self::Details => Some(Self::Welcome),
            Self::Secrets => Some(Self::Details),
            Self::Reveal => Some(Self::Secrets),
            Self::Backup => Some(Self::Reveal),
            Self::Confirm => Some(Self::Backup),
            _ => Some(Self::Wallets),
        }
    }
}
struct WalletApp {
    vault: Vault,
    stage: Stage,
    creating: bool,
    name: String,
    path: String,
    curve: Curve,
    expected: String,
    mnemonic: Zeroizing<String>,
    passphrase: Zeroizing<String>,
    repeated_passphrase: Zeroizing<String>,
    backup_confirmation: BackupConfirmation,
    no_passphrase: bool,
    pending: Option<PendingWallet>,
    revealed: PhraseReveal,
    recovery_card_height: f32,
    public_key: String,
    identifier: String,
    message: String,
    authentication_minutes: String,
    clipboard: Clipboard,
    clipboard_gate: ClipboardGate,
    secure_input: SecureInput,
    cli_import: bool,
    cli_delete: bool,
    wallets: Vec<WalletSummary>,
    search: String,
    selected_wallet: Option<String>,
    account_view: accounts::AccountView,
    sidebar_visible: bool,
    pending_wallet_selection: Option<String>,
    deletion: Option<DeletePlan>,
    cli_result: Rc<Cell<Option<Result<()>>>>,
}

struct BackupConfirmation {
    words: [Zeroizing<String>; 12],
    order: [usize; 12],
}
impl Default for BackupConfirmation {
    fn default() -> Self {
        Self {
            words: std::array::from_fn(|_| Zeroizing::new(String::new())),
            order: std::array::from_fn(|index| index),
        }
    }
}
impl BackupConfirmation {
    fn clear(&mut self) {
        for word in &mut self.words {
            word.zeroize();
        }
    }
    fn shuffle(&mut self) {
        self.clear();
        self.order = std::array::from_fn(|index| index);
        for index in (1..self.order.len()).rev() {
            let other = unsafe { libc::arc4random_uniform((index + 1) as u32) } as usize;
            self.order.swap(index, other);
        }
        if self.order.iter().copied().eq(0..12) {
            self.order.swap(0, 1);
        }
    }
    fn phrase(&self) -> Result<Zeroizing<String>> {
        let mut phrase = Zeroizing::new(String::new());
        for word in &self.words {
            let word = word.trim();
            if word.is_empty() || word.len() > 8 || !word.bytes().all(|c| c.is_ascii_alphabetic()) {
                return Err(Failure::Input);
            }
            if !phrase.is_empty() {
                phrase.push(' ');
            }
            phrase.push_str(word);
        }
        Ok(phrase)
    }
}

fn backup_fields(ui: &mut egui::Ui, confirmation: &mut BackupConfirmation) {
    let width = ((ui.available_width() - 16.0) / 3.0).floor();
    egui::Grid::new("backup-confirmation")
        .num_columns(3)
        .min_col_width(0.0)
        .spacing([8.0, 10.0])
        .show(ui, |ui| {
            for (position, index) in confirmation.order.into_iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.set_width(width);
                    secret_field(
                        ui,
                        &format!("{:02}", index + 1),
                        &format!("backup-word-{index}"),
                        &mut confirmation.words[index],
                    );
                });
                if position % 3 == 2 {
                    ui.end_row();
                }
            }
        });
}

#[derive(Default)]
struct PhraseReveal {
    phrase: Zeroizing<String>,
    deadline: Option<Instant>,
}
impl PhraseReveal {
    fn show(&mut self, phrase: String, now: Instant) {
        self.clear();
        self.phrase = Zeroizing::new(phrase);
        self.deadline = Some(now + Duration::from_secs(60));
    }
    fn clear(&mut self) {
        self.phrase.zeroize();
        self.deadline = None;
    }
    fn visible(&mut self, now: Instant, focused: bool) -> Option<&str> {
        if !focused || self.deadline.is_none_or(|deadline| now >= deadline) {
            self.clear();
            return None;
        }
        Some(&self.phrase)
    }
}

fn authentication_seconds(minutes: &str) -> Result<u64> {
    let minutes = minutes.trim();
    if minutes.is_empty() || !minutes.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Failure::Input);
    }
    minutes.parse::<u64>().ok().and_then(|value| value.checked_mul(60)).ok_or(Failure::Input)
}

fn curve_selector(ui: &mut egui::Ui, curve: &mut Curve) -> egui::Response {
    egui::Frame::new().fill(theme::SEARCH).corner_radius(8).inner_margin(3)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (value, label) in [
                    (Curve::Secp256k1, "Ethereum"),
                    (Curve::Ed25519, "Solana"),
                ] {
                    let selected = *curve == value;
                    let response = ui.add(egui::Button::new(label).selected(selected)
                        .min_size(egui::vec2(110.0, 28.0))
                        .fill(if selected { theme::SELECTED } else { theme::TRANSPARENT })
                        .stroke(egui::Stroke::NONE).corner_radius(6));
                    response.widget_info(|| egui::WidgetInfo::selected(
                        egui::WidgetType::RadioButton, ui.is_enabled(), selected, label,
                    ));
                    theme::focus_ring(ui, response.rect, response.has_focus());
                    if response.clicked() {
                        *curve = value;
                    }
                }
            });
        }).response
}

fn recovery_words(ui: &mut egui::Ui, phrase: &str, min_height: f32) {
    let columns = if ui.available_width() < 340.0 { 2 } else { 3 };
    let cell_width = ((ui.available_width() - 8.0 * (columns - 1) as f32) / columns as f32).floor();
    let cell_padding = egui::Margin::symmetric(6, 3);
    let word_width = (cell_width - cell_padding.sum().x).max(60.0);
    let rows = phrase.split_whitespace().count().div_ceil(columns).max(1) as f32;
    let row_gap = 4.0;
    let row_height = ((min_height - (rows - 1.0) * row_gap) / rows).clamp(22.0, 32.0);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, row_gap);
        ui.spacing_mut().interact_size.y = row_height;
        let mut words = phrase.split_whitespace().enumerate().peekable();
        while words.peek().is_some() {
            ui.horizontal(|ui| {
                for (i, word) in words.by_ref().take(columns) {
                    egui::Frame::new()
                        .fill(theme::CANVAS)
                        .corner_radius(6)
                        .inner_margin(cell_padding)
                        .show(ui, |ui| {
                            ui.set_width(word_width);
                            ui.set_min_height(row_height - cell_padding.sum().y);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{:02}  {}", i + 1, word))
                                        .monospace()
                                        .size(13.0),
                                )
                                .selectable(false),
                            );
                        });
                }
            });
        }
    });
}

fn revealed_card(ui: &mut egui::Ui, phrase: &str, height: f32) -> egui::Response {
    let padding = egui::Margin::symmetric(10, 12);
    let inset = padding.sum();
    let width = ui.available_width();
    egui::Frame::new().fill(theme::SOFT).corner_radius(theme::RECOVERY_BODY_CORNERS).inner_margin(padding)
        .show(ui, |ui| {
            ui.set_width((width - inset.x).max(0.0));
            ui.set_min_height((height - inset.y).max(0.0));
            recovery_words(ui, phrase, MIN_RECOVERY_CARD_HEIGHT - inset.y);
        }).response
}

fn recovery_panel(ui: &mut egui::Ui, card: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::Frame::new()
            .fill(theme::WARNING_SOFT)
            .corner_radius(egui::CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 })
            .inner_margin(egui::Margin::symmetric(10, 4))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().interact_size.y = 16.0;
                ui.horizontal(|ui| {
                    let (icon, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                    let center = icon.center();
                    let painter = ui.painter();
                    painter.rect_stroke(
                        egui::Rect::from_center_size(center - egui::vec2(0.0, 2.0), egui::vec2(8.0, 9.0)),
                        3,
                        egui::Stroke::new(1.5_f32, theme::WARNING_INK),
                        egui::StrokeKind::Inside,
                    );
                    painter.rect_filled(
                        egui::Rect::from_center_size(center + egui::vec2(0.0, 3.0), egui::vec2(12.0, 8.0)),
                        2,
                        theme::WARNING_INK,
                    );
                    ui.colored_label(
                        theme::WARNING_INK,
                        egui::RichText::new("Keep your screen private.").size(13.0).strong(),
                    );
                });
            });
        card(ui);
    });
}

fn recovery_clipboard_notice(ui: &mut egui::Ui) -> egui::InnerResponse<bool> {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clear = theme::secondary(ui, "Clear clipboard").clicked();
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(ui.available_width());
                theme::hint(ui, SEED_CLIPBOARD_NOTICE);
            });
            clear
        })
        .inner
    })
}

fn secret_field(ui: &mut egui::Ui, label: &str, id: &str, value: &mut String) -> egui::Response {
    let label = theme::label(ui, label);
    let field = theme::input_frame().show(ui, |ui| {
        TextEdit::singleline(value)
            .id(Id::new(id))
            .password(true)
            .frame(false)
            .margin(egui::Margin::ZERO)
            .desired_width(f32::INFINITY)
            .char_limit(4096)
            .show(ui)
    });
    theme::focus_ring(ui, field.response.rect, field.inner.response.has_focus());
    let mut output = field.inner;
    // Undo history would otherwise retain additional plaintext copies of secrets.
    output.state.clear_undoer();
    output.state.store(ui.ctx(), output.response.id);
    output.response.labelled_by(label.id)
}
fn filter_input(raw: &mut egui::RawInput, blocked: bool) {
    raw.dropped_files.clear();
    raw.hovered_files.clear();
    if blocked {
        raw.events
            .retain(|event| !matches!(event, Event::Copy | Event::Cut | Event::Paste(_)));
    }
}

fn saved_message(indexed: bool, output: Result<()>) -> String {
    let mut message = String::new();
    if !indexed {
        message.push_str("Wallet saved in Keychain, but the local list could not be updated. Use Sync from Keychain to repair it.");
    }
    if output.is_err() {
        if !message.is_empty() {
            message.push(' ');
        }
        message.push_str("Wallet saved in Keychain, but the command-line result could not be delivered. Do not import it again.");
    }
    message
}

fn discard_wallet_dialog(ctx: &egui::Context, creating: bool) -> Option<bool> {
    let mut choice = None;
    let response = egui::Modal::new(Id::new("discard-wallet-setup"))
        .frame(
            egui::Frame::popup(&ctx.style())
                .inner_margin(egui::Margin::same(24))
                .corner_radius(12),
        )
        .backdrop_color(egui::Color32::from_black_alpha(76))
        .show(ctx, |ui| {
            ui.set_width(320.0);
            ui.vertical_centered(|ui| {
                ui.heading(if creating { "Discard wallet setup?" } else { "Discard import?" });
            });
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if theme::secondary(ui, "Keep editing").clicked() {
                    choice = Some(false);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::danger(ui, "Discard and switch").clicked() {
                        choice = Some(true);
                    }
                });
            });
        });
    if choice.is_none() && response.should_close() { Some(false) } else { choice }
}

fn page_scroll(stage: Stage) -> egui::ScrollArea {
    egui::ScrollArea::vertical()
        .id_salt(("page-scroll", stage))
        .auto_shrink([false, false])
}

impl WalletApp {
    fn prepare_authentication(&mut self) {
        self.revealed.clear();
        // Authentication can block the UI timer; clear our sensitive copy first.
        self.clipboard.clear();
    }
    fn request(&self) -> KeyRequest {
        KeyRequest {
            name: self.name.clone(),
            path: self.path.clone(),
            curve: self.curve,
        }
    }
    fn wipe_draft(&mut self) {
        self.account_view = accounts::AccountView::default();
        self.revealed.clear();
        self.clipboard.clear();
        self.mnemonic.zeroize();
        self.passphrase.zeroize();
        self.repeated_passphrase.zeroize();
        self.backup_confirmation.clear();
        self.pending = None;
        self.pending_wallet_selection = None;
    }
    fn reveal_wallet(&mut self, name: &str) -> Result<()> {
        self.wipe_draft();
        self.prepare_authentication();
        self.secure_input.set(true)?;
        let context = macos::AuthContext::new(
            &format!("show the seed phrase for wallet \"{name}\""), false,
        );
        let mut wallet = match self.vault.read(name, &context) {
            Ok(wallet) => wallet,
            Err(error) => {
                let _ = self.secure_input.set(false);
                return Err(error);
            }
        };
        self.revealed.show(std::mem::take(&mut wallet.mnemonic), Instant::now());
        self.name = name.to_owned();
        self.message.clear();
        Ok(())
    }
    fn back(&mut self) {
        self.show_wallets();
    }
    fn navigate_back(&mut self) {
        let Some(previous) = self.stage.previous() else {
            self.revealed.clear();
            self.sidebar_visible = true;
            return;
        };
        // Keep the unsaved backup when stepping back within its confirmation flow.
        if matches!(self.stage, Stage::Backup | Stage::Confirm) {
            self.clipboard.clear();
            self.backup_confirmation.clear();
        } else {
            self.wipe_draft();
        }
        self.message.clear();
        self.stage = previous;
        if previous == Stage::Wallets {
            self.show_wallets();
        }
    }
    fn return_to_wallets(&mut self) {
        self.wipe_draft();
        self.deletion = None;
        self.message.clear();
        self.stage = Stage::Wallets;
    }
    fn select_wallet(&mut self, name: String) {
        if self.stage.has_draft() {
            self.revealed.clear();
            self.clipboard.clear();
            self.pending_wallet_selection = Some(name);
        } else {
            self.return_to_wallets();
            self.selected_wallet = Some(name);
        }
    }
    fn show_wallets(&mut self) {
        self.return_to_wallets();
        self.wallets.clear();
        match Catalog::for_group(&self.vault.group).and_then(|c| c.load()) {
            Ok(entries) => self.wallets = entries,
            Err(error) => {
                self.message = format!(
                "Unable to load the local name list ({}). Use Sync from Keychain to rebuild it.",
                error.code()
            )
            }
        }
    }
    fn sync_wallets(&mut self) {
        self.prepare_authentication();
        match Catalog::for_group(&self.vault.group).and_then(|c| c.sync(|| self.vault.summaries()))
        {
            Ok(entries) => {
                self.wallets = entries;
                self.message.clear();
            }
            Err(error) => {
                self.message = format!(
                    "Name synchronization stopped ({}). The previous list was kept.",
                    error.code()
                )
            }
        }
    }
    fn confirm_delete(&mut self, names: Vec<String>) {
        self.revealed.clear();
        match DeletePlan::new(names) {
            Ok(plan) => {
                self.deletion = Some(plan);
                self.message.clear();
                self.stage = Stage::Delete;
            }
            Err(error) => self.message = format!("Unable to prepare deletion ({}).", error.code()),
        }
    }
    fn prepare(&mut self) -> Result<()> {
        let request = self.request();
        request.validate()?;
        check_passphrase(
            &self.passphrase,
            &self.repeated_passphrase,
            self.no_passphrase,
        )?;
        let words = if self.creating {
            macos::generate_mnemonic()?
        } else {
            normalize(&self.mnemonic)
        };
        let wallet = StoredWallet {
            version: 1,
            mnemonic: words.to_string(),
            passphrase: self.passphrase.to_string(),
        };
        let pair = derive(
            &wallet,
            &request,
            if self.creating {
                None
            } else {
                Some(&self.expected)
            },
        )?;
        self.public_key = pair.public_key;
        self.identifier = pair.identifier;
        self.mnemonic.zeroize();
        self.passphrase.zeroize();
        self.repeated_passphrase.zeroize();
        self.pending = Some(PendingWallet::new(wallet, self.creating));
        if self.creating {
            self.stage = Stage::Reveal;
            Ok(())
        } else {
            self.save()
        }
    }
    fn save(&mut self) -> Result<()> {
        self.prepare_authentication();
        let wallet = self.pending.as_ref().ok_or(Failure::Input)?.for_save()?;
        let summary = WalletSummary::from_wallet(&self.name, wallet)?;
        self.vault.insert(&self.name, wallet)?;
        let indexed = Catalog::for_group(&self.vault.group)
            .and_then(|c| c.upsert(summary.clone()))
            .is_ok();
        self.wallets.retain(|entry| entry.name != summary.name);
        self.wallets.push(summary);
        self.wallets.sort_by(|a, b| a.name.cmp(&b.name));
        self.wipe_draft();
        let metadata = if self.curve == Curve::Secp256k1 {
            serde_json::json!({"name":self.name,"curve":self.curve.name(),"path":self.path,"publicKey":self.public_key,"address":self.identifier})
        } else {
            serde_json::json!({"name":self.name,"curve":self.curve.name(),"path":self.path,"publicKey":self.public_key})
        };
        let output = if self.cli_import {
            crate::emit(&metadata)
        } else {
            Ok(())
        };
        self.message = saved_message(indexed, output);
        self.stage = Stage::Success;
        self.search.clear();
        self.selected_wallet = Some(self.name.clone());
        self.cli_result.set(Some(output));
        Ok(())
    }
    fn title(&self) -> &'static str {
        match self.stage {
            Stage::Welcome => "Add wallet",
            Stage::Wallets => "Wallets",
            Stage::Settings => "Settings",
            Stage::Details => {
                if self.creating {
                    "New wallet"
                } else {
                    "Import wallet"
                }
            }
            Stage::Secrets => {
                if self.creating {
                    "Create wallet"
                } else {
                    "Import wallet"
                }
            }
            Stage::Reveal | Stage::Backup => "Back up seed phrase",
            Stage::Confirm => "Verify backup",
            Stage::ViewPhrase => "Seed phrase",
            Stage::Success => {
                if self.creating {
                    "Wallet created"
                } else {
                    "Wallet imported"
                }
            }
            Stage::Delete => "Confirm deletion",
            Stage::Fatal => "Unable to open the wallet",
        }
    }
    fn content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, focused: bool) {
        match self.stage {
            Stage::Welcome => {
                let mut choice = None;
                let card_width = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).floor();
                ui.horizontal(|ui| {
                    if theme::wallet_choice(ui, card_width, true).clicked() {
                        choice = Some(true);
                    }
                    if theme::wallet_choice(ui, card_width, false).clicked() {
                        choice = Some(false);
                    }
                });
                if let Some(creating) = choice {
                    self.creating = creating;
                    self.no_passphrase = false;
                    self.name.clear();
                    self.expected.clear();
                    self.curve = Curve::Secp256k1;
                    self.path = DEFAULT_ETH_PATH.into();
                    self.message.clear();
                    self.stage = Stage::Details;
                }
            }
            Stage::Wallets => {
                if !self.message.is_empty() {
                    ui.colored_label(theme::DANGER_INK, &self.message);
                    ui.add_space(8.0);
                }
                self.wallet_detail(ui);
            }
            Stage::Settings => {
                ui.horizontal(|ui| {
                    let label = theme::label(ui, "Reuse Node auth");
                    let minutes = ui.add(
                        TextEdit::singleline(&mut self.authentication_minutes)
                            .desired_width(48.0)
                            .margin(egui::Margin::symmetric(8, 6))
                            .char_limit(20),
                    );
                    let unit = ui.label("min");
                    theme::focus_ring(ui, minutes.rect, minutes.has_focus());
                    minutes.labelled_by(label.id).labelled_by(unit.id);
                });
                theme::hint(ui, "0 prompts every time.");
                ui.add_space(12.0);
                if theme::primary(ui, "Save").clicked() {
                    match authentication_seconds(&self.authentication_minutes) {
                        Ok(seconds) => match Catalog::for_group(&self.vault.group)
                            .and_then(|catalog| catalog.set_authentication_ttl(seconds)) {
                                Ok(()) => { self.message.clear(); self.back(); }
                                Err(_) => self.message = "Unable to save settings.".into(),
                            },
                        Err(_) => self.message = "Enter a valid whole number of minutes (0 or greater).".into(),
                    }
                }
            }
            Stage::Details => {
                let previous = self.curve;
                curve_selector(ui, &mut self.curve);
                ui.add_space(8.0);
                if self.curve != previous {
                    self.expected.clear();
                    self.path = if self.curve == Curve::Secp256k1 {
                        DEFAULT_ETH_PATH.into()
                    } else {
                        DEFAULT_SOL_PATH.into()
                    };
                }
                theme::public_field(ui, "Wallet name", &mut self.name, 64);
                theme::public_field(
                    ui,
                    if self.curve == Curve::Ed25519 {
                        "Hardened derivation path"
                    } else {
                        "Derivation path"
                    },
                    &mut self.path,
                    128,
                );
                if !self.creating {
                    theme::public_field(
                        ui,
                        if self.curve == Curve::Secp256k1 {
                            "Original Ethereum address"
                        } else {
                            "Original Ed25519 public key (0x hex, 32 bytes)"
                        },
                        &mut self.expected,
                        66,
                    );
                }
                ui.add_space(12.0);
                if theme::primary(ui, "Continue").clicked() {
                    let result = if self.creating {
                        self.request().validate()
                    } else {
                        self.request().validate_identifier(&self.expected)
                    };
                    match result {
                        Ok(()) => {
                            self.message.clear();
                            self.stage = Stage::Secrets;
                        }
                        Err(e) => self.message = e.message().into(),
                    }
                }
            }
            Stage::Secrets => {
                if self.cli_import {
                    theme::hint(ui, &format!("{} · {}", self.curve.name(), self.path));
                }
                if focused {
                    if !self.creating {
                        secret_field(
                            ui,
                            "English seed phrase",
                            "mnemonic",
                            &mut self.mnemonic,
                        );
                    }
                    secret_field(
                        ui,
                        "BIP-39 passphrase (optional)",
                        "passphrase",
                        &mut self.passphrase,
                    );
                    secret_field(
                        ui,
                        "Confirm BIP-39 passphrase",
                        "repeat-passphrase",
                        &mut self.repeated_passphrase,
                    );
                    ui.checkbox(&mut self.no_passphrase, "I use no BIP-39 passphrase");
                    theme::hint(ui, "A BIP-39 passphrase changes the wallet. Spaces count.");
                } else {
                    ui.label("Focus this window to enter wallet secrets.");
                }
                let (next, cancel) = theme::actions(
                    ui,
                    "Cancel",
                    if self.creating {
                        "Continue to backup"
                    } else {
                        "Verify and save"
                    },
                );
                if next && focused {
                    if let Err(error) = self.prepare() {
                        self.message = error.message().into();
                    }
                }
                if cancel {
                    if self.cli_import {
                        self.cli_result.set(Some(Err(Failure::Cancelled)));
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                    self.back();
                }
            }
            Stage::Reveal => {
                ui.label("Reveal in private. Keep all 12 words in order and offline. Anyone with the phrase can access your funds.");
                let (reveal, cancel) = theme::actions(ui, "Cancel", "Reveal seed phrase");
                if reveal {
                    self.message.clear();
                    self.stage = Stage::Backup;
                }
                if cancel {
                    self.back();
                }
            }
            Stage::Backup => {
                if focused {
                    if let Some(pending) = &self.pending {
                        recovery_words(ui, pending.phrase(), 0.0);
                        ui.add_space(16.0);
                        if theme::secondary(
                            ui,
                            if self.clipboard.has_copy() {
                                "Copied"
                            } else {
                                "Copy seed phrase"
                            },
                        )
                        .clicked()
                        {
                            match self.clipboard.copy(pending.phrase()) {
                                Ok(()) => ctx.request_repaint(),
                                Err(e) => self.message = e.message().into(),
                            }
                        }
                    }
                    if self.clipboard.has_copy() {
                        theme::hint(ui, SEED_CLIPBOARD_NOTICE);
                    }
                } else {
                    ui.label("Seed phrase hidden while this window is inactive.");
                }
                let (next, cancel) =
                    theme::actions(ui, "Cancel", "Continue");
                if next && focused {
                    self.message.clear();
                    self.clipboard.clear();
                    self.backup_confirmation.shuffle();
                    self.stage = Stage::Confirm;
                }
                if cancel {
                    self.back();
                }
            }
            Stage::Confirm => {
                if focused {
                    ui.label("Match each word to its number.");
                    ui.add_space(8.0);
                    backup_fields(ui, &mut self.backup_confirmation);
                } else {
                    ui.label("Focus this window to enter your backup.");
                }
                ui.add_space(16.0);
                let (save, skip, cancel) = theme::backup_actions(ui);
                if (save || skip) && focused {
                    let confirmation = if skip {
                        self.pending.as_mut().ok_or(Failure::Input).map(|p| p.skip_backup())
                    } else {
                        self.backup_confirmation.phrase()
                            .and_then(|phrase| self.pending.as_mut().ok_or(Failure::Input)?.confirm(&phrase))
                    };
                    let result = confirmation.and_then(|_| self.save());
                    if let Err(e) = result {
                        self.message = if e == Failure::Input && !skip {
                            "Check each word against its number. Nothing was saved.".into()
                        } else {
                            e.message().into()
                        };
                    }
                }
                if cancel {
                    self.back();
                }
            }
            Stage::Success => {
                theme::hint(
                    ui,
                    &format!("{} · {}", self.curve.name(), self.path),
                );
                theme::label(ui, "Public key");
                ui.label(egui::RichText::new(&self.public_key).monospace());
                if self.curve == Curve::Secp256k1 {
                    theme::label(ui, "Address");
                    ui.label(egui::RichText::new(&self.identifier).monospace());
                }
                if self.cli_import {
                    if theme::primary(ui, "Done").clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                } else if theme::primary(ui, "Back to wallets").clicked() {
                    self.back();
                }
            }
            Stage::ViewPhrase => {
                recovery_panel(ui, |ui| self.recovery_card(ui, &self.name.clone()));
                ui.add_space(6.0);
                let shown = self.revealed.visible(Instant::now(), focused).is_some();
                let (copy, hide) = theme::recovery_actions(ui, shown, self.clipboard.has_copy());
                if copy {
                    if let Some(phrase) = self.revealed.visible(Instant::now(), focused) {
                        match self.clipboard.copy(phrase) {
                            Ok(()) => {
                                self.message.clear();
                                ctx.request_repaint();
                            }
                            Err(_) => self.message = "Unable to copy the seed phrase.".into(),
                        }
                    }
                }
                if hide {
                    self.revealed.clear();
                    ctx.request_repaint();
                }
                if self.clipboard.has_copy() {
                    let notice = recovery_clipboard_notice(ui);
                    if notice.inner {
                        self.clipboard.clear();
                    } else if copy {
                        notice.response.scroll_to_me(None);
                    }
                }
            }
            Stage::Delete => {
                if let Some(plan) = &mut self.deletion {
                    if plan.deleted > 0 {
                        ui.label(format!("{} remaining · {} deleted", plan.remaining.len(), plan.deleted));
                    }
                    for name in &plan.remaining {
                        ui.label(egui::RichText::new(name).monospace());
                    }
                    theme::hint(ui, "Each wallet requires authentication. If you stop, earlier deletions still apply.");
                    ui.checkbox(
                        &mut plan.confirmed,
                        "I have an offline backup and understand deletion is permanent.",
                    );
                }
                ui.separator();
                let mut cancel = false;
                let mut delete = false;
                ui.horizontal(|ui| {
                    cancel = theme::secondary(ui, "Cancel").clicked();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready = focused && self.deletion.as_ref().is_some_and(|p| p.confirmed);
                        delete = ui
                            .add_enabled_ui(ready, |ui| {
                                theme::danger(ui, "Authenticate and delete")
                            })
                            .inner
                            .clicked();
                    });
                });
                if cancel {
                    if self.cli_delete {
                        self.cli_result.set(Some(Err(Failure::Cancelled)));
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    } else {
                        self.return_to_wallets();
                    }
                }
                if delete && focused {
                    self.prepare_authentication();
                    let vault = &self.vault;
                    let wallets = &mut self.wallets;
                    let mut index_failed = false;
                    let result = self
                        .deletion
                        .as_mut()
                        .ok_or(Failure::Input)
                        .and_then(|plan| {
                            plan.execute(|name| {
                                vault.delete_authenticated(name)?;
                                wallets.retain(|entry| entry.name != name);
                                if Catalog::for_group(&vault.group)
                                    .and_then(|c| c.remove(name))
                                    .is_err()
                                {
                                    index_failed = true;
                                }
                                Ok(())
                            })
                        });
                    match result {
                            Ok(()) if self.cli_delete => {
                                match crate::emit(&serde_json::json!({"name":self.name})) {
                                    Ok(()) => { self.cli_result.set(Some(Ok(()))); ctx.send_viewport_cmd(ViewportCommand::Close); }
                                    Err(error) => { self.cli_result.set(Some(Err(error))); ctx.send_viewport_cmd(ViewportCommand::Close); }
                                }
                            }
                            Ok(()) => self.return_to_wallets(),
                            Err(error) => self.message = format!("Deletion stopped ({}). Review the remaining wallets before trying again.", error.code()),
                        }
                    if index_failed {
                        self.message.push_str(" A wallet was deleted from Keychain, but the local name list could not be updated. Use Sync from Keychain to repair it.");
                    }
                }
            }
            Stage::Fatal => {
                ui.label(&self.message);
                if theme::secondary(ui, "Quit").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
        if !matches!(self.stage, Stage::Fatal | Stage::Wallets) && !self.message.is_empty() {
            ui.add_space(12.0);
            egui::Frame::new()
                .fill(theme::DANGER_SOFT)
                .corner_radius(8)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.colored_label(theme::DANGER_INK, &self.message);
                });
        }
    }

    fn page_content(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, focused: bool) {
        page_scroll(self.stage)
            .show(ui, |ui| {
                let max_width = if self.stage == Stage::ViewPhrase { 440.0 } else { 420.0 };
                let width = ui.available_width().min(max_width);
                ui.vertical_centered(|ui| {
                    ui.set_width(width);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        if self.stage != Stage::Wallets {
                            let wallet = match self.stage {
                                Stage::ViewPhrase | Stage::Success => Some(self.name.as_str()),
                                Stage::Secrets if self.cli_import => Some(self.name.as_str()),
                                _ => None,
                            };
                            theme::page_header(ui, self.title(), wallet);
                        }
                        self.content(ui, ctx, focused && self.pending_wallet_selection.is_none());
                    });
                });
            });
    }
}

impl eframe::App for WalletApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::CANVAS.to_normalized_gamma_f32()
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        filter_input(raw, self.stage.blocks_clipboard());
    }
    fn persist_egui_memory(&self) -> bool {
        false
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let focused = ctx.input(|i| i.focused);
        if self.stage != Stage::ViewPhrase {
            self.revealed.clear();
        }
        let phrase_visible = self.revealed.visible(Instant::now(), focused).is_some();
        let contains_secrets = phrase_visible || matches!(
            self.stage,
            Stage::Secrets | Stage::Reveal | Stage::Backup | Stage::Confirm
        );
        if let Err(error) = self.secure_input.set(focused && contains_secrets) {
            self.wipe_draft();
            self.cli_result.set(Some(Err(error)));
            self.message = error.message().into();
            self.stage = Stage::Fatal;
        }
        self.clipboard.tick(Instant::now());
        ctx.request_repaint_after(Duration::from_millis(250));
        if !self.cli_import && !self.cli_delete {
            egui::TopBottomPanel::top("navigation")
                .show_separator_line(false)
                .frame(egui::Frame::new()
                    .fill(theme::CANVAS)
                    .inner_margin(egui::Margin::symmetric(12, 6)))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.set_min_height(28.0);
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let can_back = self.stage.previous().is_some()
                            || self.stage == Stage::Wallets && !self.sidebar_visible;
                        if ui.add_enabled_ui(can_back, |ui| theme::navigation_button(ui, "Back", None))
                            .inner.clicked() {
                            self.navigate_back();
                        }
                        if theme::navigation_button(ui,
                            if self.sidebar_visible { "Hide sidebar" } else { "Show sidebar" }, Some(self.sidebar_visible)).clicked() {
                            self.sidebar_visible = !self.sidebar_visible;
                        }
                        if self.stage == Stage::Wallets {
                            ui.add_space(6.0);
                            theme::label(ui, self.title());
                            self.wallet_controls(ui);
                        }
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::CANVAS)
                    .inner_margin(if self.cli_import || self.cli_delete {
                        theme::PAGE_MARGIN
                    } else {
                        egui::Margin::ZERO
                    }),
            )
            .show(ctx, |ui| {
                if self.cli_import || self.cli_delete {
                    self.page_content(ui, ctx, focused);
                } else {
                    self.wallet_browser(ui, ctx, focused);
                }
            });
        if self.pending_wallet_selection.is_some() {
            match discard_wallet_dialog(ctx, self.creating) {
                Some(true) => {
                    let name = self.pending_wallet_selection.take().unwrap();
                    self.return_to_wallets();
                    self.selected_wallet = Some(name);
                }
                Some(false) => self.pending_wallet_selection = None,
                None => {}
            }
        }
        self.clipboard_gate.set_blocked(self.stage.blocks_clipboard());
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.wipe_draft();
        self.clipboard.clear();
        let _ = self.secure_input.set(false);
    }
}

pub fn run(
    vault: Vault,
    import: Option<(KeyRequest, String)>,
    delete: Option<String>,
) -> Result<()> {
    let gate = ClipboardGate::new()?;
    let cli_mode = import.is_some() || delete.is_some();
    let outcome = Rc::new(Cell::new(None));
    let mut app = WalletApp {
        vault,
        stage: Stage::Welcome,
        creating: false,
        name: String::new(),
        path: DEFAULT_ETH_PATH.into(),
        curve: Curve::Secp256k1,
        expected: String::new(),
        mnemonic: Zeroizing::new(String::new()),
        passphrase: Zeroizing::new(String::new()),
        repeated_passphrase: Zeroizing::new(String::new()),
        backup_confirmation: BackupConfirmation::default(),
        no_passphrase: false,
        pending: None,
        revealed: PhraseReveal::default(),
        recovery_card_height: MIN_RECOVERY_CARD_HEIGHT,
        public_key: String::new(),
        identifier: String::new(),
        message: String::new(),
        authentication_minutes: String::new(),
        clipboard: Clipboard::new(),
        clipboard_gate: gate,
        secure_input: SecureInput::new(),
        cli_import: import.is_some(),
        cli_delete: delete.is_some(),
        wallets: Vec::new(),
        search: String::new(),
        selected_wallet: None,
        account_view: accounts::AccountView::default(),
        sidebar_visible: true,
        pending_wallet_selection: None,
        deletion: None,
        cli_result: outcome.clone(),
    };
    if let Some((request, expected)) = import {
        app.name = request.name;
        app.path = request.path;
        app.curve = request.curve;
        app.expected = expected;
        app.stage = Stage::Secrets;
    }
    if let Some(name) = delete {
        app.name = name.clone();
        app.confirm_delete(vec![name]);
    }
    if !cli_mode {
        app.show_wallets();
    }
    app.clipboard_gate.set_blocked(app.stage.blocks_clipboard());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 440.0])
            .with_min_inner_size([640.0, 400.0])
            .with_title_shown(false)
            .with_drag_and_drop(false),
        ..Default::default()
    };
    eframe::run_native(
        "Keylet",
        options,
        Box::new(move |cc| {
            theme::configure(&cc.egui_ctx);
            cc.egui_ctx.send_viewport_cmd(ViewportCommand::SetTheme(egui::SystemTheme::Light));
            let color = theme::CANVAS;
            let _ = macos::configure_window_background([color.r(), color.g(), color.b()]);
            Ok(Box::new(app))
        }),
    )
    .map_err(|_| Failure::Configuration)?;
    if cli_mode {
        outcome.take().unwrap_or(Err(Failure::Cancelled))
    } else {
        outcome.take().unwrap_or(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn opening_settings_does_not_inherit_seed_phrase_scroll() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(304.0, 344.0));
        let render_page = |stage, scroll| {
            let mut input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            if scroll {
                input.events.extend([
                    Event::PointerMoved(egui::pos2(150.0, 200.0)),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -400.0),
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
            }
            let mut top = 0.0;
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    page_scroll(stage).show(ui, |ui| {
                        top = ui.label("Page heading").rect.top();
                        if stage == Stage::ViewPhrase {
                            ui.allocate_space(egui::vec2(1.0, 600.0));
                        }
                    });
                });
            });
            top
        };
        let settings_top = render_page(Stage::Settings, false);
        let phrase_top = render_page(Stage::ViewPhrase, false);
        for _ in 0..8 {
            render_page(Stage::ViewPhrase, true);
        }
        assert!(render_page(Stage::ViewPhrase, false) < phrase_top);
        assert_eq!(render_page(Stage::Settings, false), settings_top);
    }

    #[test]
    fn curve_segments_fit_the_form_and_switch_both_ways() {
        for width in [280.0, 420.0] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut curve = Curve::Secp256k1;
            let mut bounds = egui::Rect::NOTHING;
            for target in [None, Some(Curve::Ed25519), Some(Curve::Secp256k1)] {
                let mut input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO, egui::vec2(width + 32.0, 100.0),
                    )),
                    ..Default::default()
                };
                if let Some(target) = target {
                    let pos = egui::pos2(bounds.left() + bounds.width()
                        * if target == Curve::Ed25519 { 0.75 } else { 0.25 }, bounds.center().y);
                    input.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        input.events.push(egui::Event::PointerButton {
                            pos, button: egui::PointerButton::Primary,
                            pressed, modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(16))
                        .show(ctx, |ui| {
                            bounds = curve_selector(ui, &mut curve).rect;
                            assert!(ui.max_rect().contains_rect(bounds));
                        });
                });
                if let Some(target) = target {
                    assert!(curve == target);
                }
            }
        }
    }

    #[test]
    fn shuffled_backup_requires_every_word_at_its_original_number() {
        let phrase = "abandon amount liar amount expire adjust cage candy arch gather drum buyer";
        let mut confirmation = BackupConfirmation::default();
        for _ in 0..24 {
            confirmation.shuffle();
            let mut indices = confirmation.order;
            indices.sort_unstable();
            assert_eq!(indices, std::array::from_fn(|index| index));
            assert_ne!(confirmation.order, indices);
            assert!(confirmation.phrase().is_err());
        }
        let order = confirmation.order;
        for index in order {
            confirmation.words[index].push_str(phrase.split_whitespace().nth(index).unwrap());
        }
        assert_eq!(confirmation.order, order);
        let mut pending = PendingWallet::new(StoredWallet {
            version: 1, mnemonic: phrase.into(), passphrase: String::new(),
        }, true);
        assert!(pending.for_save().is_err());
        assert!(pending.confirm(&confirmation.phrase().unwrap()).is_ok());
        assert!(pending.for_save().is_ok());
        confirmation.words.swap(0, 1);
        assert!(pending.confirm(&confirmation.phrase().unwrap()).is_err());
        assert!(pending.for_save().is_err());
        for input in ["", "two words", "word1", "toooolong"] {
            confirmation.words[0].zeroize();
            confirmation.words[0].push_str(input);
            assert!(confirmation.phrase().is_err());
        }
        confirmation.shuffle();
        assert!(confirmation.words.iter().all(|word| word.is_empty()));
    }

    #[test]
    fn twelve_backup_fields_and_actions_fit_compact_page() {
        for width in [272.0, 360.0, 420.0] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut confirmation = BackupConfirmation::default();
            confirmation.shuffle();
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO, egui::vec2(width + 48.0, 360.0),
                )),
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(24))
                    .show(ctx, |ui| {
                        let bounds = ui.max_rect();
                        theme::page_header(ui, "Verify backup", None);
                        ui.label("Match each word to its number.");
                        ui.add_space(8.0);
                        let grid = ui.scope(|ui| backup_fields(ui, &mut confirmation));
                        assert!(bounds.contains_rect(grid.response.rect), "width={width}, grid={:?}, bounds={bounds:?}", grid.response.rect);
                        ui.add_space(16.0);
                        let actions = ui.scope(theme::backup_actions);
                        assert!(bounds.contains_rect(actions.response.rect), "width={width}, actions={:?}, bounds={bounds:?}", actions.response.rect);
                    });
            });
        }
    }

    #[test]
    fn toolbar_back_follows_creation_steps_and_returns_auxiliary_pages_to_wallets() {
        use super::Stage;
        for (from, to) in [
            (Stage::Details, Stage::Welcome), (Stage::Secrets, Stage::Details),
            (Stage::Reveal, Stage::Secrets), (Stage::Backup, Stage::Reveal),
            (Stage::Confirm, Stage::Backup), (Stage::ViewPhrase, Stage::Wallets),
            (Stage::Settings, Stage::Wallets), (Stage::Delete, Stage::Wallets),
        ] {
            assert!(from.previous() == Some(to));
        }
        assert!(Stage::Wallets.previous().is_none());
        assert!(Stage::Fatal.previous().is_none());
    }
    #[test]
    fn custom_authentication_minutes_accept_long_windows_and_reject_overflow() {
        for (input, seconds) in [("0", 0), ("5", 300), ("90", 5400), ("1440", 86400)] {
            assert_eq!(super::authentication_seconds(input).unwrap(), seconds);
        }
        let largest = u64::MAX / 60;
        assert_eq!(super::authentication_seconds(&largest.to_string()).unwrap(), largest * 60);
        for input in ["".to_owned(), "-1".into(), "1.5".into(), "abc".into(), (largest + 1).to_string()] {
            assert!(super::authentication_seconds(&input).is_err());
        }
    }
    use super::*;
    #[test]
    fn only_unfinished_wallet_setup_requires_discard_confirmation() {
        for stage in [Stage::Details, Stage::Secrets, Stage::Reveal, Stage::Backup, Stage::Confirm] {
            assert!(stage.has_draft());
        }
        for stage in [Stage::Welcome, Stage::Wallets, Stage::Settings, Stage::Success,
            Stage::ViewPhrase, Stage::Delete, Stage::Fatal] {
            assert!(!stage.has_draft());
        }
    }
    #[test]
    fn discard_confirmation_centers_the_title_and_splits_the_actions() {
        for (creating, title) in [(false, "Discard import?"), (true, "Discard wallet setup?")] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            ctx.style_mut(|style| style.animation_time = 0.0);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 400.0));
            let render = || {
                ctx.run(egui::RawInput { screen_rect: Some(screen), ..Default::default() }, |ctx| {
                    discard_wallet_dialog(ctx, creating);
                })
            };
            let _ = render();
            let output = render();
            let button = |fill| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect),
                    _ => None,
                }).expect("dialog action should be visible")
            };
            let keep = button(theme::CONTROL);
            let discard = button(theme::DANGER);
            let center = screen.center().x;
            assert!((keep.left() - (center - 160.0)).abs() < 1.0, "{title}: {keep:?}");
            assert!((discard.right() - (center + 160.0)).abs() < 1.0, "{title}: {discard:?}");
            assert!((keep.center().y - discard.center().y).abs() < 1.0);
            let title_x = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == title => {
                    Some(text.pos.x + text.galley.rect.center().x)
                }
                _ => None,
            }).expect("dialog title should be visible");
            assert!((title_x - center).abs() < 1.0, "{title}: center x is {title_x}");
        }
    }
    #[test]
    fn escape_from_discard_confirmation_keeps_the_wallet_draft() {
        for creating in [false, true] {
            let ctx = egui::Context::default();
            for escape in [false, true] {
                let mut input = egui::RawInput::default();
                if escape {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::Escape, physical_key: None, pressed: true,
                        repeat: false, modifiers: egui::Modifiers::NONE,
                    });
                }
                let _ = ctx.run(input, |ctx| {
                    assert_eq!(discard_wallet_dialog(ctx, creating), if escape { Some(false) } else { None });
                });
            }
        }
    }
    #[test]
    fn saved_wallet_reports_delivery_failure_without_claiming_save_failed() {
        assert!(saved_message(true, Ok(())).is_empty());
        let message = saved_message(true, Err(Failure::Transport));
        assert!(message.contains("Wallet saved in Keychain"));
        assert!(message.contains("Do not import it again"));
        assert!(!message.contains("Nothing was saved"));
        let message = saved_message(false, Err(Failure::Transport));
        assert!(message.contains("Sync from Keychain"));
        assert!(message.contains("could not be delivered"));
    }
    #[test]
    fn hiding_phrase_keeps_card_and_footer_in_place() {
        for width in [260.0, 356.0, 360.0, 400.0, 452.0] {
            for count in [12, 24] {
                let ctx = egui::Context::default();
                theme::configure(&ctx);
                let mut height = MIN_RECOVERY_CARD_HEIGHT;
                let mut footer_top: Option<f32> = None;
                for revealed in [true, false, true] {
                    let output = ctx.run(egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO, egui::vec2(width + 32.0, 900.0),
                        )),
                        ..Default::default()
                    }, |ctx| {
                        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(16))
                            .show(ctx, |ui| {
                                let bounds = ui.max_rect();
                                let card = if revealed {
                                    revealed_card(ui, &"abstract ".repeat(count), height)
                                } else {
                                    theme::recovery_mask(ui, height)
                                };
                                assert!(bounds.contains_rect(card.rect), "width={width} count={count} revealed={revealed} bounds={bounds:?} card={:?}", card.rect);
                                if width >= 400.0 && count == 12 {
                                    assert!((card.rect.height() - MIN_RECOVERY_CARD_HEIGHT).abs() < 0.5);
                                }
                                height = card.rect.height();
                                ui.add_space(16.0);
                                let footer = ui.scope(|ui| theme::recovery_actions(ui, revealed, false));
                                assert!((footer.response.rect.left() - card.rect.left()).abs() < 0.5);
                                assert!((footer.response.rect.right() - card.rect.right()).abs() < 0.5);
                                if let Some(top) = footer_top {
                                    assert!((footer.response.rect.top() - top).abs() < 0.5, "width={width} count={count} revealed={revealed} previous={top} current={} height={height}", footer.response.rect.top());
                                }
                                footer_top = Some(footer.response.rect.top());
                            });
                    });
                    for (label, on_left) in [("Copy", true), ("Hide", false)] {
                        let x = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => {
                                Some(text.pos.x + text.galley.size().x / 2.0)
                            }
                            _ => None,
                        }).expect("recovery action should be visible");
                        assert_eq!(x < (width + 32.0) / 2.0, on_left, "{label} at width {width}");
                    }
                }
            }
        }
    }

    #[test]
    fn copied_seed_phrase_notice_is_visible_at_supported_sizes() {
        for (width, height, words, scroll) in [
            (260.0, 360.0, 12, false),
            (272.0, 360.0, 12, false),
            (359.0, 360.0, 12, false),
            (360.0, 360.0, 12, false),
            (420.0, 360.0, 12, false),
            (440.0, 360.0, 12, false),
            (420.0, 400.0, 24, false),
            (260.0, 360.0, 24, true),
        ] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut visible = false;
            for frame in 0..if scroll { 24 } else { 1 } {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width + 48.0, height),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::new().inner_margin(theme::PAGE_MARGIN))
                            .show(ctx, |ui| {
                                let page = page_scroll(Stage::ViewPhrase).show(ui, |ui| {
                                    theme::page_header(ui, "Seed phrase", Some(&"wallet-name-".repeat(5)));
                                    recovery_panel(ui, |ui| {
                                        revealed_card(ui, &"abstract ".repeat(words), MIN_RECOVERY_CARD_HEIGHT);
                                    });
                                    ui.add_space(6.0);
                                    theme::recovery_actions(ui, true, true);
                                    let notice = recovery_clipboard_notice(ui);
                                    if scroll && frame == 0 {
                                        notice.response.scroll_to_me(None);
                                    }
                                    notice.response.rect
                                });
                                assert!(page.content_size.x <= width + 0.5);
                                visible = page.inner_rect.contains_rect(page.inner);
                            });
                    },
                );
                if visible {
                    break;
                }
            }
            assert!(visible, "{words} words at {width} × {height}: notice outside the viewport");
        }
    }

    #[test]
    fn revealed_phrase_expires_and_cannot_reappear_after_focus_loss() {
        let now = Instant::now();
        let mut reveal = PhraseReveal::default();
        assert!(reveal.visible(now, true).is_none());
        reveal.show("public display fixture".into(), now);
        assert_eq!(reveal.visible(now + Duration::from_secs(59), true), Some("public display fixture"));
        assert!(reveal.visible(now + Duration::from_secs(60), true).is_none());
        assert!(reveal.phrase.is_empty());
        assert!(reveal.deadline.is_none());

        reveal.show("another public fixture".into(), now);
        assert!(reveal.visible(now, false).is_none());
        assert!(reveal.phrase.is_empty());
        assert!(reveal.visible(now, true).is_none());

        reveal.show("public display fixture".into(), now);
        reveal.clear();
        assert!(reveal.phrase.is_empty());
        assert!(reveal.visible(now, true).is_none());
    }

    #[test]
    fn recovery_grid_fits_twelve_and_twenty_four_words() {
        for width in [360.0, 400.0, 452.0] {
            for count in [12, 24] {
                let ctx = egui::Context::default();
                theme::configure(&ctx);
                let raw = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO, egui::vec2(width + 32.0, 600.0),
                    )),
                    ..Default::default()
                };
                let _ = ctx.run(raw, |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(16))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            let grid = ui.scope(|ui| recovery_words(ui, &"abstract ".repeat(count), 0.0));
                            assert!(bounds.contains_rect(grid.response.rect), "width={width}, count={count}, bounds={bounds:?}, grid={:?}", grid.response.rect);
                        });
                });
            }
        }
    }

    #[test]
    fn clipboard_is_allowed_on_public_pages_and_blocked_on_secret_pages() {
        for (stage, blocked) in [
            (Stage::Welcome, false), (Stage::Wallets, false),
            (Stage::Settings, false), (Stage::Details, false),
            (Stage::Secrets, true), (Stage::Reveal, true),
            (Stage::Backup, true), (Stage::Confirm, true),
            (Stage::Success, false), (Stage::ViewPhrase, true),
            (Stage::Delete, false), (Stage::Fatal, false),
        ] {
            assert_eq!(stage.blocks_clipboard(), blocked);
            let events = vec![
                Event::Copy, Event::Cut, Event::Paste("public sentinel".into()),
                Event::Text("t".into()),
            ];
            let mut raw = egui::RawInput {
                events: events.clone(),
                dropped_files: vec![egui::DroppedFile::default()],
                hovered_files: vec![egui::HoveredFile::default()],
                ..Default::default()
            };
            filter_input(&mut raw, stage.blocks_clipboard());
            assert_eq!(raw.events, if blocked { vec![Event::Text("t".into())] } else { events });
            assert!(raw.dropped_files.is_empty());
            assert!(raw.hovered_files.is_empty());

            let ctx = egui::Context::default();
            let mut text = String::new();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(TextEdit::singleline(&mut text).id(Id::new("clipboard-field"))
                        .password(blocked)).request_focus();
                });
            });
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(TextEdit::singleline(&mut text).id(Id::new("clipboard-field"))
                        .password(blocked));
                });
            });
            assert_eq!(text, if blocked { "t" } else { "public sentinelt" });
        }
    }
}
