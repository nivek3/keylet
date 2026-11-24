mod accounts;
mod wallets;

use crate::catalog::Catalog;
use crate::theme::{self, WalletChoice};
use crate::{
    macos::{self, Clipboard, ClipboardGate, SecureInput, Vault},
    model::{
        Curve, DEFAULT_ETH_PATH, DEFAULT_SOL_PATH, DeletePlan, Failure, KeyRequest, PendingWallet,
        Result, StoredWallet, WalletSummary, check_passphrase, derive, normalize,
    },
};
use eframe::egui::{self, Event, Id, TextEdit, ViewportCommand};
use std::{
    cell::Cell,
    io::Write,
    rc::Rc,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

const SEED_CLIPBOARD_NOTICE: &str = "Clears after 60s while open. Other apps may keep a copy.";
/// The phrase card starts at its content's height, then keeps its revealed height so
/// hiding the words never moves the actions below.
const MIN_RECOVERY_CARD_HEIGHT: f32 = 0.0;
/// Height the word grid's rows are sized against.
const WORD_GRID_HEIGHT: f32 = 156.0;
const DELETE_INDEX_WARNING: &str = "A wallet was deleted from Keychain, but the wallet list could not be updated. Use Sync from Keychain to repair it.";

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
        matches!(
            self,
            Self::Secrets | Self::Reveal | Self::Backup | Self::Confirm | Self::ViewPhrase
        )
    }

    fn has_draft(self) -> bool {
        matches!(
            self,
            Self::Details | Self::Secrets | Self::Reveal | Self::Backup | Self::Confirm
        )
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
    signer_authentication_minutes: String,
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
    copied: Option<&'static str>,
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
            // SAFETY: arc4random_uniform has no preconditions; the bound is at most 12.
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
    minutes
        .parse::<u64>()
        .ok()
        .and_then(|value| value.checked_mul(60))
        .ok_or(Failure::Input)
}

fn curve_selector(ui: &mut egui::Ui, curve: &mut Curve) -> egui::Response {
    egui::Frame::new()
        .fill(theme::SELECTED)
        .corner_radius(8)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (value, label) in [(Curve::Secp256k1, "Ethereum"), (Curve::Ed25519, "Solana")] {
                    let selected = *curve == value;
                    let response = ui.add(
                        egui::Button::new(if selected {
                            egui::RichText::new(label).font(theme::medium_font(13.0))
                        } else {
                            egui::RichText::new(label)
                        })
                        .selected(selected)
                        .min_size(egui::vec2(110.0, 24.0))
                        .fill(if selected {
                            theme::SURFACE
                        } else {
                            theme::TRANSPARENT
                        })
                        .stroke(if selected {
                            egui::Stroke::new(1.0_f32, theme::BORDER)
                        } else {
                            egui::Stroke::NONE
                        })
                        .corner_radius(6),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::RadioButton,
                            ui.is_enabled(),
                            selected,
                            label,
                        )
                    });
                    theme::focus_ring(ui, response.rect, response.has_focus());
                    if response.clicked() {
                        *curve = value;
                    }
                }
            });
        })
        .response
}

fn recovery_words(ui: &mut egui::Ui, phrase: &str, min_height: f32, color: egui::Color32) {
    let width = ui.available_width();
    let columns = if width < 340.0 {
        2
    } else if width < 520.0 {
        3
    } else {
        4
    };
    let cell_width = ((width - 8.0 * (columns - 1) as f32) / columns as f32).floor();
    let cell_padding = egui::Margin::symmetric(8, 4);
    let word_width = (cell_width - cell_padding.sum().x).max(60.0);
    let rows = phrase.split_whitespace().count().div_ceil(columns).max(1) as f32;
    let row_gap = 6.0;
    let row_height = ((min_height - (rows - 1.0) * row_gap) / rows).clamp(24.0, 32.0);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, row_gap);
        // Controls inside a cell fill the row minus the cell's padding.
        ui.spacing_mut().interact_size.y = row_height - cell_padding.sum().y;
        let mut words = phrase.split_whitespace().enumerate().peekable();
        while words.peek().is_some() {
            ui.horizontal(|ui| {
                for (i, word) in words.by_ref().take(columns) {
                    egui::Frame::new()
                        .fill(theme::SOFT)
                        .corner_radius(6)
                        .inner_margin(cell_padding)
                        .show(ui, |ui| {
                            ui.set_width(word_width);
                            ui.set_min_height(row_height - cell_padding.sum().y);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.0;
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("{:02}", i + 1))
                                            .monospace()
                                            .size(11.0)
                                            .color(theme::MUTED),
                                    )
                                    .selectable(false),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(word)
                                            .monospace()
                                            .size(13.0)
                                            .color(color),
                                    )
                                    .selectable(false),
                                );
                            });
                        });
                }
            });
        }
    });
}

/// The backup steps' button row: an optional extra action on the leading edge;
/// Cancel and the primary action on the trailing edge. Returns `(primary, cancel)`.
fn backup_buttons(
    ui: &mut egui::Ui,
    secondary: Option<(&str, bool, &mut bool)>,
    primary: &str,
) -> (bool, bool) {
    let leading: Vec<theme::Action<'_>> = secondary
        .as_ref()
        .map(|(label, enabled, _)| theme::Action::secondary(label).enabled(*enabled))
        .into_iter()
        .collect();
    let (lead, trail) = theme::button_bar(
        ui,
        &leading,
        &[
            theme::Action::secondary("Cancel"),
            theme::Action::primary(primary),
        ],
    );
    if let Some((_, _, clicked)) = secondary {
        *clicked = lead[0];
    }
    (trail[1], trail[0])
}

/// The seed phrase card body: the words once revealed, otherwise numbered
/// placeholders that reveal the phrase when clicked. `height` keeps the card the
/// same size across reveal and hide.
fn phrase_card(ui: &mut egui::Ui, phrase: Option<&str>, height: f32) -> egui::Response {
    let padding = egui::Margin::same(14);
    let inset = padding.sum();
    let width = ui.available_width();
    let mask = ["••••••"; 12].join(" ");
    let (words, color) = match phrase {
        Some(phrase) => (phrase, theme::INK),
        None => (mask.as_str(), theme::MUTED),
    };
    let body = egui::Frame::new()
        .inner_margin(padding)
        .show(ui, |ui| {
            ui.set_width((width - inset.x).max(0.0));
            ui.set_min_height((height - inset.y).max(0.0));
            recovery_words(ui, words, WORD_GRID_HEIGHT, color);
        })
        .response;
    if phrase.is_some() {
        return body;
    }
    let response = ui.interact(body.rect, Id::new("seed-phrase-mask"), egui::Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(body.rect.shrink(3.0), 8, egui::Color32::from_black_alpha(8));
    }
    theme::focus_ring(ui, body.rect.shrink(1.0), response.has_focus());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            "Show seed phrase",
        )
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PhraseAction {
    Show,
    Copy,
    Hide,
    ClearClipboard,
}

/// The phrase page's button row: Clear clipboard on the leading edge once copied;
/// Hide and Copy while shown, or Show while masked, on the trailing edge.
fn phrase_actions(ui: &mut egui::Ui, shown: bool, copied: bool) -> Option<PhraseAction> {
    use theme::Action;
    fn buttons<'a>(list: &[(theme::Action<'a>, PhraseAction)]) -> Vec<theme::Action<'a>> {
        list.iter().map(|(action, _)| *action).collect()
    }
    let leading: Vec<(Action<'_>, PhraseAction)> = if copied {
        vec![(
            Action::secondary("Clear clipboard"),
            PhraseAction::ClearClipboard,
        )]
    } else {
        Vec::new()
    };
    let trailing = if shown {
        vec![
            (Action::secondary("Hide"), PhraseAction::Hide),
            (
                Action::primary(if copied { "Copied" } else { "Copy" }),
                PhraseAction::Copy,
            ),
        ]
    } else {
        vec![(Action::primary("Show"), PhraseAction::Show)]
    };
    let (lead, trail) = theme::button_bar(ui, &buttons(&leading), &buttons(&trailing));
    leading
        .iter()
        .zip(lead)
        .chain(trailing.iter().zip(trail))
        .find_map(|((_, action), clicked)| clicked.then_some(*action))
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
        message.push_str("Wallet saved in Keychain, but the wallet list could not be updated. Use Sync from Keychain to repair it.");
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
                .inner_margin(egui::Margin::same(22))
                .corner_radius(14),
        )
        .backdrop_color(egui::Color32::from_black_alpha(76))
        .show(ctx, |ui| {
            ui.set_width(380.0);
            theme::alert(
                ui,
                |ui| {
                    let choice = if creating {
                        theme::WalletChoice::Create
                    } else {
                        theme::WalletChoice::Import
                    };
                    let glyph = if creating { "+" } else { "↓" };
                    theme::alert_icon(ui, theme::choice_tile(choice), glyph, None);
                },
                if creating {
                    "Discard wallet setup?"
                } else {
                    "Discard import?"
                },
                "The details you entered for this wallet won\u{2019}t be kept.",
                |ui| {
                    let (discard, keep) =
                        theme::alert_buttons(ui, "Keep editing", "Discard", true, true);
                    if keep {
                        choice = Some(false);
                    }
                    if discard {
                        choice = Some(true);
                    }
                },
            );
        });
    if choice.is_none() && response.should_close() {
        Some(false)
    } else {
        choice
    }
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
            &format!("show the seed phrase for wallet \"{name}\""),
            false,
        );
        let mut wallet = match self.vault.read(name, &context) {
            Ok(wallet) => wallet,
            Err(error) => {
                let _ = self.secure_input.set(false);
                return Err(error);
            }
        };
        self.revealed
            .show(std::mem::take(&mut wallet.mnemonic), Instant::now());
        name.clone_into(&mut self.name);
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
                    "Unable to load the wallet list ({}). Use Sync from Keychain to rebuild it.",
                    error.code()
                );
            }
        }
    }

    fn sync_wallets(&mut self) {
        self.prepare_authentication();
        // Keychain rebuilds the derived caches and re-derives remembered paths.
        let summaries = || self.vault.summaries(&self.wallets);
        match Catalog::for_group(&self.vault.group).and_then(|c| c.sync(summaries)) {
            Ok(entries) => {
                self.wallets = entries;
                self.message.clear();
            }
            Err(Failure::CatalogChanged) => {
                self.message =
                    "The wallet list changed during sync. Run Sync from Keychain again.".into();
            }
            Err(error) => {
                self.message = format!(
                    "Sync stopped ({}). The previous list was kept.",
                    error.code()
                );
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
        self.pending = Some(PendingWallet::new(wallet, self.creating));
        if self.creating {
            self.mnemonic.zeroize();
            self.passphrase.zeroize();
            self.repeated_passphrase.zeroize();
            self.stage = Stage::Reveal;
            Ok(())
        } else {
            // Keep the form inputs for retry and drop the extra prepared copy.
            let result = self.save();
            self.pending = None;
            result
        }
    }

    fn save(&mut self) -> Result<()> {
        self.prepare_authentication();
        let wallet = self.pending.as_ref().ok_or(Failure::Input)?.for_save()?;
        let summary = WalletSummary::from_wallet(&self.name, wallet, &[])?;
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
            Stage::Details | Stage::Secrets => {
                if self.creating {
                    "New wallet"
                } else {
                    "Import wallet"
                }
            }
            Stage::Reveal | Stage::Backup | Stage::ViewPhrase => "Seed phrase",
            Stage::Confirm => "Verify backup",
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
                let card_width =
                    ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).floor();
                ui.horizontal(|ui| {
                    if theme::wallet_choice(ui, card_width, WalletChoice::Create).clicked() {
                        choice = Some(WalletChoice::Create);
                    }
                    if theme::wallet_choice(ui, card_width, WalletChoice::Import).clicked() {
                        choice = Some(WalletChoice::Import);
                    }
                });
                if let Some(choice) = choice {
                    self.creating = matches!(choice, WalletChoice::Create);
                    self.no_passphrase = false;
                    self.name.clear();
                    self.expected.clear();
                    self.curve = Curve::Secp256k1;
                    self.path = DEFAULT_ETH_PATH.into();
                    self.message.clear();
                    self.stage = Stage::Details;
                }
            }
            Stage::Wallets => self.wallet_detail(ui),
            Stage::Settings => {
                theme::group(ui, |ui| {
                    theme::form_row(ui, "Reuse Node auth", |ui| {
                        theme::text_field(ui, &mut self.authentication_minutes, 20, 64.0);
                        ui.label(egui::RichText::new("minutes").color(theme::MUTED));
                    });
                    theme::form_row(ui, "Reuse signer auth", |ui| {
                        theme::text_field(ui, &mut self.signer_authentication_minutes, 20, 64.0);
                        ui.label(egui::RichText::new("minutes").color(theme::MUTED));
                    });
                });
                theme::footnote(
                    ui,
                    "How long a Node.js process or signer session may reuse your authentication. A signer session signs any payload during this time. 0 asks every time.",
                );
                ui.add_space(14.0);
                let (_, save) = theme::button_bar(ui, &[], &[theme::Action::primary("Save")]);
                if save[0] {
                    match (
                        authentication_seconds(&self.authentication_minutes),
                        authentication_seconds(&self.signer_authentication_minutes),
                    ) {
                        (Ok(node), Ok(signer)) => match self
                            .vault
                            .set_authentication_ttl(node)
                            .and_then(|()| self.vault.set_signer_authentication_ttl(signer))
                        {
                            Ok(()) => {
                                self.message.clear();
                                self.back();
                            }
                            Err(_) => self.message = "Unable to save settings.".into(),
                        },
                        _ => self.message = "Enter whole minutes, 0 or more.".into(),
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
                        "Hardened path"
                    } else {
                        "Path"
                    },
                    &mut self.path,
                    128,
                );
                if !self.creating {
                    theme::public_field(
                        ui,
                        if self.curve == Curve::Secp256k1 {
                            "Ethereum address"
                        } else {
                            "Ed25519 public key (hex)"
                        },
                        &mut self.expected,
                        66,
                    );
                }
                ui.add_space(12.0);
                let (_, next) = theme::button_bar(ui, &[], &[theme::Action::primary("Continue")]);
                if next[0] {
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
                        secret_field(ui, "English seed phrase", "mnemonic", &mut self.mnemonic);
                    }
                    secret_field(
                        ui,
                        "Passphrase (optional)",
                        "passphrase",
                        &mut self.passphrase,
                    );
                    secret_field(
                        ui,
                        "Confirm passphrase",
                        "repeat-passphrase",
                        &mut self.repeated_passphrase,
                    );
                    ui.checkbox(&mut self.no_passphrase, "No passphrase");
                    theme::hint(ui, "A BIP-39 passphrase changes the wallet. Spaces count.");
                } else {
                    ui.label("Focus this window to enter wallet secrets.");
                }
                let (next, cancel) = theme::actions(
                    ui,
                    "Cancel",
                    if self.creating { "Continue" } else { "Import" },
                );
                if next
                    && focused
                    && let Err(error) = self.prepare()
                {
                    self.message = error.message().into();
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
                // The words stay masked until asked for, like the wallet's Seed phrase page.
                let card = theme::group(ui, |ui| phrase_card(ui, None, MIN_RECOVERY_CARD_HEIGHT));
                theme::footnote(
                    ui,
                    if self.clipboard.has_copy() {
                        SEED_CLIPBOARD_NOTICE
                    } else {
                        "Write the words down in order and keep them offline. \
                         Anyone with them can take your funds."
                    },
                );
                ui.add_space(16.0);
                let (reveal, cancel) = backup_buttons(ui, None, "Show");
                if reveal || card.inner.clicked() {
                    self.message.clear();
                    self.stage = Stage::Backup;
                }
                if cancel {
                    self.back();
                }
            }
            Stage::Backup => {
                let phrase = self
                    .pending
                    .as_ref()
                    .filter(|_| focused)
                    .map(super::model::PendingWallet::phrase);
                theme::group(ui, |ui| phrase_card(ui, phrase, MIN_RECOVERY_CARD_HEIGHT));
                let hide = theme::footnote_action(
                    ui,
                    if !focused {
                        "Seed phrase hidden while this window is inactive."
                    } else if self.clipboard.has_copy() {
                        SEED_CLIPBOARD_NOTICE
                    } else {
                        "Write the words down in order and keep them offline."
                    },
                    "Hide",
                )
                .clicked();
                ui.add_space(16.0);
                let copy_label = if self.clipboard.has_copy() {
                    "Copied"
                } else {
                    "Copy"
                };
                let (next, cancel, copy) = {
                    let mut copy = false;
                    let (next, cancel) =
                        backup_buttons(ui, Some((copy_label, focused, &mut copy)), "Continue");
                    (next, cancel, copy)
                };
                if copy
                    && focused
                    && let Some(pending) = &self.pending
                {
                    match self.clipboard.copy(pending.phrase()) {
                        Ok(()) => ctx.request_repaint(),
                        Err(e) => self.message = e.message().into(),
                    }
                }
                if next && focused {
                    self.message.clear();
                    self.clipboard.clear();
                    self.backup_confirmation.shuffle();
                    self.stage = Stage::Confirm;
                }
                if hide {
                    // Masks the words only; a copy stays until its 60-second clear.
                    self.message.clear();
                    self.stage = Stage::Reveal;
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
                        self.pending
                            .as_mut()
                            .ok_or(Failure::Input)
                            .map(super::model::PendingWallet::skip_backup)
                    } else {
                        self.backup_confirmation.phrase().and_then(|phrase| {
                            self.pending
                                .as_mut()
                                .ok_or(Failure::Input)?
                                .confirm(&phrase)
                        })
                    };
                    let result = confirmation.and_then(|()| self.save());
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
                theme::hint(ui, &format!("{} · {}", self.curve.name(), self.path));
                theme::label(ui, "Public key");
                ui.label(egui::RichText::new(&self.public_key).monospace());
                if self.curve == Curve::Secp256k1 {
                    theme::label(ui, "Address");
                    ui.label(egui::RichText::new(&self.identifier).monospace());
                }
                ui.add_space(12.0);
                let (_, done) = theme::button_bar(ui, &[], &[theme::Action::primary("Done")]);
                if done[0] {
                    if self.cli_import {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    } else {
                        self.back();
                    }
                }
            }
            Stage::ViewPhrase => {
                let name = self.name.clone();
                theme::detail_header(ui, &name, "Seed phrase");
                ui.add_space(14.0);
                let card_clicked = theme::group(ui, |ui| self.recovery_card(ui)).inner;
                theme::footnote(
                    ui,
                    "Anyone with these words can take your funds. Keep your screen private.",
                );
                ui.add_space(14.0);
                let shown = self.revealed.visible(Instant::now(), focused).is_some();
                let copied = self.clipboard.has_copy();
                let action = phrase_actions(ui, shown, copied);
                if copied {
                    let notice = theme::footnote(ui, SEED_CLIPBOARD_NOTICE);
                    let scroll = Id::new("scroll-seed-notice");
                    if ui
                        .data_mut(|data| data.remove_temp::<bool>(scroll))
                        .unwrap_or(false)
                    {
                        notice.scroll_to_me(None);
                    }
                }
                let (show, copy, hide, clear) = (
                    card_clicked || action == Some(PhraseAction::Show),
                    action == Some(PhraseAction::Copy),
                    action == Some(PhraseAction::Hide),
                    action == Some(PhraseAction::ClearClipboard),
                );
                if show
                    && focused
                    && let Err(error) = self.reveal_wallet(&name)
                {
                    self.message = format!("Seed phrase not shown ({}).", error.code());
                }
                if clear {
                    self.clipboard.clear();
                }
                if copy && let Some(phrase) = self.revealed.visible(Instant::now(), focused) {
                    match self.clipboard.copy(phrase) {
                        Ok(()) => {
                            self.message.clear();
                            ui.data_mut(|data| {
                                data.insert_temp(Id::new("scroll-seed-notice"), true);
                            });
                            ctx.request_repaint();
                        }
                        Err(_) => self.message = "Unable to copy the seed phrase.".into(),
                    }
                }
                if hide {
                    self.revealed.clear();
                    ctx.request_repaint();
                }
            }
            Stage::Delete => {
                let mut cancel = false;
                let mut delete = false;
                if let Some(plan) = &mut self.deletion {
                    let single = plan.remaining.len() == 1;
                    let title = match plan.remaining.as_slice() {
                        [name] => format!("Delete \u{201c}{name}\u{201d}?"),
                        names => format!("Delete {} wallets?", names.len()),
                    };
                    let body = if single {
                        "Its seed phrase will be removed from Keychain on this Mac. \
                         Only your offline backup can restore it."
                    } else {
                        "Their seed phrases will be removed from Keychain on this Mac. \
                         Each wallet asks for authentication; if you stop, earlier deletions still apply."
                    };
                    let first = plan.remaining.first().cloned().unwrap_or_default();
                    theme::alert(
                        ui,
                        |ui| {
                            if single {
                                theme::wallet_alert_icon(ui, &first, Some(theme::DANGER));
                            } else {
                                let count = plan.remaining.len().to_string();
                                theme::alert_icon(ui, theme::MUTED, &count, Some(theme::DANGER));
                            }
                        },
                        &title,
                        body,
                        |ui| {
                            if plan.deleted > 0 {
                                theme::hint(
                                    ui,
                                    &format!(
                                        "{} deleted · {} remaining",
                                        plan.deleted,
                                        plan.remaining.len()
                                    ),
                                );
                            }
                            if !single {
                                for name in &plan.remaining {
                                    ui.label(egui::RichText::new(name).color(theme::MUTED));
                                }
                            }
                            ui.checkbox(
                                &mut plan.confirmed,
                                "I have an offline backup and understand deletion is permanent.",
                            );
                            let ready = focused && plan.confirmed;
                            (delete, cancel) =
                                theme::alert_buttons(ui, "Cancel", "Delete", true, ready);
                        },
                    );
                }
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
                            if index_failed {
                                let _ = writeln!(std::io::stderr(), "{DELETE_INDEX_WARNING}");
                            }
                            let output = crate::emit(&serde_json::json!({"name":self.name}));
                            self.cli_result.set(Some(output));
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        Ok(()) => self.return_to_wallets(),
                        Err(error) => {
                            self.message = format!(
                                "Deletion stopped ({}). Review the remaining wallets before trying again.",
                                error.code()
                            );
                        }
                    }
                    if index_failed && !self.cli_delete {
                        if !self.message.is_empty() {
                            self.message.push(' ');
                        }
                        self.message.push_str(DELETE_INDEX_WARNING);
                    }
                }
            }
            Stage::Fatal => {
                ui.label(&self.message);
                let (_, quit) = theme::button_bar(ui, &[], &[theme::Action::secondary("Quit")]);
                if quit[0] {
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
        if self.stage == Stage::Wallets && !self.message.is_empty() {
            ui.colored_label(theme::DANGER_INK, &self.message);
            ui.add_space(8.0);
        }
        page_scroll(self.stage).show(ui, |ui| {
            let max_width = match self.stage {
                Stage::Wallets | Stage::ViewPhrase => 760.0,
                Stage::Delete => 520.0,
                Stage::Reveal | Stage::Backup => 560.0,
                _ => 420.0,
            };
            let width = ui.available_width().min(max_width);
            ui.vertical_centered(|ui| {
                ui.set_width(width);
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    // Wallet pages draw their own header with the wallet's icon.
                    if !matches!(
                        self.stage,
                        Stage::Wallets | Stage::ViewPhrase | Stage::Delete
                    ) {
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
        let contains_secrets = phrase_visible
            || matches!(
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
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(
                if self.cli_import || self.cli_delete {
                    theme::PAGE_MARGIN
                } else {
                    egui::Margin::ZERO
                },
            ))
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
                    if let Some(name) = self.pending_wallet_selection.take() {
                        self.return_to_wallets();
                        self.selected_wallet = Some(name);
                    }
                }
                Some(false) => self.pending_wallet_selection = None,
                None => {}
            }
        }
        self.clipboard_gate
            .set_blocked(self.stage.blocks_clipboard());
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
        signer_authentication_minutes: String::new(),
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
        copied: None,
        pending_wallet_selection: None,
        deletion: None,
        cli_result: Rc::clone(&outcome),
    };
    if let Some((request, expected)) = import {
        app.name = request.name;
        app.path = request.path;
        app.curve = request.curve;
        app.expected = expected;
        app.stage = Stage::Secrets;
    }
    if let Some(name) = delete {
        app.name.clone_from(&name);
        app.confirm_delete(vec![name]);
    }
    if !cli_mode {
        app.show_wallets();
    }
    app.clipboard_gate.set_blocked(app.stage.blocks_clipboard());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(if cli_mode {
                [640.0, 440.0]
            } else {
                [860.0, 600.0]
            })
            .with_min_inner_size(if cli_mode {
                [640.0, 400.0]
            } else {
                [640.0, 420.0]
            })
            // The sidebar runs under the transparent title bar, as in Passwords.
            .with_fullsize_content_view(!cli_mode)
            .with_title_shown(false)
            .with_drag_and_drop(false)
            // eframe otherwise replaces the Dock icon with its default egui icon.
            // An empty icon (decode failure) falls back to the bundle's AppIcon.icns.
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Keylet",
        options,
        Box::new(move |cc| {
            theme::configure(&cc.egui_ctx);
            cc.egui_ctx
                .send_viewport_cmd(ViewportCommand::SetTheme(egui::SystemTheme::Light));
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

/// The Keylet icon, decoded from the same PNG the bundle's AppIcon.icns is built from.
fn app_icon() -> egui::IconData {
    const PNG: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/app-icon.png"));
    eframe::icon_data::from_png_bytes(PNG).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_app_icon_decodes() {
        let icon = super::app_icon();
        assert_eq!((icon.width, icon.height), (1024, 1024));
        assert_eq!(icon.rgba.len(), 1024 * 1024 * 4);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the same page must lay out at exactly the same offset"
    )]
    fn opening_settings_does_not_inherit_seed_phrase_scroll() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(304.0, 344.0));
        let render_page = |stage, scroll| {
            let mut input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
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
                        egui::Pos2::ZERO,
                        egui::vec2(width + 32.0, 100.0),
                    )),
                    ..Default::default()
                };
                if let Some(target) = target {
                    let pos = egui::pos2(
                        bounds.left()
                            + bounds.width() * if target == Curve::Ed25519 { 0.75 } else { 0.25 },
                        bounds.center().y,
                    );
                    input.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        input.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let _ = ctx.run(input, |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(16))
                        .show(ctx, |ui| {
                            bounds = curve_selector(ui, &mut curve).rect;
                            assert!(ui.max_rect().contains_rect(bounds));
                        });
                });
                if let Some(target) = target {
                    assert_eq!(curve, target);
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
        let mut pending = PendingWallet::new(
            StoredWallet {
                version: 1,
                mnemonic: phrase.into(),
                passphrase: String::new(),
            },
            true,
        );
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
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width + 48.0, 360.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(24))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            theme::page_header(ui, "Verify backup", None);
                            ui.label("Match each word to its number.");
                            ui.add_space(8.0);
                            let grid = ui.scope(|ui| backup_fields(ui, &mut confirmation));
                            assert!(
                                bounds.contains_rect(grid.response.rect),
                                "width={width}, grid={:?}, bounds={bounds:?}",
                                grid.response.rect
                            );
                            ui.add_space(16.0);
                            let actions = ui.scope(theme::backup_actions);
                            assert!(
                                bounds.contains_rect(actions.response.rect),
                                "width={width}, actions={:?}, bounds={bounds:?}",
                                actions.response.rect
                            );
                        });
                },
            );
        }
    }

    #[test]
    fn toolbar_back_follows_creation_steps_and_returns_auxiliary_pages_to_wallets() {
        use super::Stage;
        for (from, to) in [
            (Stage::Details, Stage::Welcome),
            (Stage::Secrets, Stage::Details),
            (Stage::Reveal, Stage::Secrets),
            (Stage::Backup, Stage::Reveal),
            (Stage::Confirm, Stage::Backup),
            (Stage::ViewPhrase, Stage::Wallets),
            (Stage::Settings, Stage::Wallets),
            (Stage::Delete, Stage::Wallets),
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
        assert_eq!(
            super::authentication_seconds(&largest.to_string()).unwrap(),
            largest * 60
        );
        for input in [
            String::new(),
            "-1".into(),
            "1.5".into(),
            "abc".into(),
            (largest + 1).to_string(),
        ] {
            assert!(super::authentication_seconds(&input).is_err());
        }
    }
    use super::*;
    #[test]
    fn only_unfinished_wallet_setup_requires_discard_confirmation() {
        for stage in [
            Stage::Details,
            Stage::Secrets,
            Stage::Reveal,
            Stage::Backup,
            Stage::Confirm,
        ] {
            assert!(stage.has_draft());
        }
        for stage in [
            Stage::Welcome,
            Stage::Wallets,
            Stage::Settings,
            Stage::Success,
            Stage::ViewPhrase,
            Stage::Delete,
            Stage::Fatal,
        ] {
            assert!(!stage.has_draft());
        }
    }

    #[test]
    fn discard_confirmation_uses_the_alert_layout() {
        for (creating, title) in [(false, "Discard import?"), (true, "Discard wallet setup?")] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            ctx.style_mut(|style| style.animation_time = 0.0);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 400.0));
            let render = || {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ctx| {
                        discard_wallet_dialog(ctx, creating);
                    },
                )
            };
            let _ = render();
            let output = render();
            let button = |fill| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if rect.fill == fill => Some(rect.rect),
                        _ => None,
                    })
                    .expect("dialog action should be visible")
            };
            let keep = button(theme::SECONDARY);
            let discard = button(theme::DANGER);
            let center = screen.center().x;
            // The destructive default sits at the trailing edge, Keep editing before it.
            assert!(
                (discard.right() - (center + 190.0)).abs() < 1.0,
                "{title}: {discard:?}"
            );
            assert!(keep.right() < discard.left(), "{title}: {keep:?}");
            assert!((keep.center().y - discard.center().y).abs() < 1.0);
            let title_left = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == title => Some(text.pos.x),
                    _ => None,
                })
                .expect("dialog title should be visible");
            // The title starts to the right of the 64pt icon.
            assert!(
                title_left > center - 190.0 + 64.0,
                "{title}: title at {title_left}"
            );
        }
    }

    #[test]
    fn escape_from_discard_confirmation_keeps_the_wallet_draft() {
        for creating in [false, true] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            for escape in [false, true] {
                let mut input = egui::RawInput::default();
                if escape {
                    input.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                let _ = ctx.run(input, |ctx| {
                    assert_eq!(
                        discard_wallet_dialog(ctx, creating),
                        if escape { Some(false) } else { None }
                    );
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
    fn hiding_phrase_keeps_card_and_actions_in_place() {
        for width in [260.0, 356.0, 400.0, 560.0, 760.0] {
            for count in [12, 24] {
                let ctx = egui::Context::default();
                theme::configure(&ctx);
                let mut height = MIN_RECOVERY_CARD_HEIGHT;
                let mut actions_top: Option<f32> = None;
                for revealed in [true, false, true] {
                    let phrase = "abstract ".repeat(count);
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width + 32.0, 900.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default()
                                .frame(egui::Frame::new().inner_margin(16))
                                .show(ctx, |ui| {
                                    let bounds = ui.max_rect();
                                    let card = theme::group(ui, |ui| {
                                        phrase_card(ui, revealed.then_some(phrase.as_str()), height)
                                    })
                                    .inner;
                                    assert!(
                                        bounds.contains_rect(card.rect),
                                        "width={width} count={count} card={:?}",
                                        card.rect
                                    );
                                    if !revealed {
                                        assert!(
                                            (card.rect.height() - height).abs() < 0.5,
                                            "hidden card should keep the revealed height"
                                        );
                                    }
                                    if revealed {
                                        height = card.rect.height();
                                    }
                                    ui.add_space(14.0);
                                    let actions =
                                        ui.scope(|ui| phrase_actions(ui, revealed, false));
                                    let top = actions.response.rect.top();
                                    if let Some(previous) = actions_top {
                                        assert!(
                                            (top - previous).abs() < 0.5,
                                            "width={width} count={count} revealed={revealed}"
                                        );
                                    }
                                    actions_top = Some(top);
                                });
                        },
                    );
                }
            }
        }
    }

    #[test]
    fn backup_buttons_stay_on_one_row() {
        for width in [392.0, 420.0, 560.0] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width + 32.0, 200.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(16))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            let mut copy = false;
                            let row = ui.scope(|ui| {
                                backup_buttons(ui, Some(("Copy", true, &mut copy)), "Continue")
                            });
                            assert!(bounds.contains_rect(row.response.rect), "{width}");
                        });
                },
            );
            let center_y = |label: &str| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos.y + text.galley.size().y / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("{label} missing at {width}"))
            };
            let row = center_y("Cancel");
            for label in ["Copy", "Continue"] {
                assert!((center_y(label) - row).abs() < 1.0, "{label} at {width}");
            }
        }
    }

    #[test]
    fn copied_seed_phrase_notice_is_visible_at_supported_sizes() {
        for (width, height, words, scroll) in [
            (392.0, 420.0, 12, true),
            (560.0, 560.0, 12, false),
            (760.0, 560.0, 12, false),
            (560.0, 420.0, 24, true),
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
                                    theme::detail_header(
                                        ui,
                                        &"wallet-name-".repeat(5),
                                        "Seed phrase",
                                    );
                                    ui.add_space(14.0);
                                    theme::group(ui, |ui| {
                                        phrase_card(
                                            ui,
                                            Some(&"abstract ".repeat(words)),
                                            MIN_RECOVERY_CARD_HEIGHT,
                                        )
                                    });
                                    theme::footnote(ui, "Keep your screen private.");
                                    ui.add_space(14.0);
                                    phrase_actions(ui, true, true);
                                    let notice = theme::footnote(ui, SEED_CLIPBOARD_NOTICE);
                                    if scroll && frame == 0 {
                                        notice.scroll_to_me(None);
                                    }
                                    notice.rect
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
            assert!(
                visible,
                "{words} words at {width} × {height}: notice outside the viewport"
            );
        }
    }

    #[test]
    fn revealed_phrase_expires_and_cannot_reappear_after_focus_loss() {
        let now = Instant::now();
        let mut reveal = PhraseReveal::default();
        assert!(reveal.visible(now, true).is_none());
        reveal.show("public display fixture".into(), now);
        assert_eq!(
            reveal.visible(now + Duration::from_secs(59), true),
            Some("public display fixture")
        );
        assert!(
            reveal
                .visible(now + Duration::from_secs(60), true)
                .is_none()
        );
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
                        egui::Pos2::ZERO,
                        egui::vec2(width + 32.0, 600.0),
                    )),
                    ..Default::default()
                };
                let _ = ctx.run(raw, |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(16))
                        .show(ctx, |ui| {
                            let bounds = ui.max_rect();
                            let grid = ui.scope(|ui| {
                                recovery_words(ui, &"abstract ".repeat(count), 0.0, theme::INK);
                            });
                            assert!(
                                bounds.contains_rect(grid.response.rect),
                                "width={width}, count={count}, bounds={bounds:?}, grid={:?}",
                                grid.response.rect
                            );
                        });
                });
            }
        }
    }

    #[test]
    fn clipboard_is_allowed_on_public_pages_and_blocked_on_secret_pages() {
        for (stage, blocked) in [
            (Stage::Welcome, false),
            (Stage::Wallets, false),
            (Stage::Settings, false),
            (Stage::Details, false),
            (Stage::Secrets, true),
            (Stage::Reveal, true),
            (Stage::Backup, true),
            (Stage::Confirm, true),
            (Stage::Success, false),
            (Stage::ViewPhrase, true),
            (Stage::Delete, false),
            (Stage::Fatal, false),
        ] {
            assert_eq!(stage.blocks_clipboard(), blocked);
            let events = vec![
                Event::Copy,
                Event::Cut,
                Event::Paste("public sentinel".into()),
                Event::Text("t".into()),
            ];
            let mut raw = egui::RawInput {
                events: events.clone(),
                dropped_files: vec![egui::DroppedFile::default()],
                hovered_files: vec![egui::HoveredFile::default()],
                ..Default::default()
            };
            filter_input(&mut raw, stage.blocks_clipboard());
            assert_eq!(
                raw.events,
                if blocked {
                    vec![Event::Text("t".into())]
                } else {
                    events
                }
            );
            assert!(raw.dropped_files.is_empty());
            assert!(raw.hovered_files.is_empty());

            let ctx = egui::Context::default();
            let mut text = String::new();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(
                        TextEdit::singleline(&mut text)
                            .id(Id::new("clipboard-field"))
                            .password(blocked),
                    )
                    .request_focus();
                });
            });
            let _ = ctx.run(raw, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add(
                        TextEdit::singleline(&mut text)
                            .id(Id::new("clipboard-field"))
                            .password(blocked),
                    );
                });
            });
            assert_eq!(text, if blocked { "t" } else { "public sentinelt" });
        }
    }
}
