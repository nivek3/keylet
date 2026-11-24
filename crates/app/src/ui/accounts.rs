use super::{WalletApp, curve_selector};
use crate::catalog::Catalog;
use crate::{
    macos,
    model::{Curve, DEFAULT_ETH_PATH, Failure, KeyRequest, Result, WalletSeed, WalletSummary},
    theme,
};
use eframe::egui;

pub(super) struct AccountView {
    wallet: String,
    curve: Curve,
    full_path: bool,
    index: String,
    path: String,
    address: Option<String>,
    /// Public addresses cached for this wallet's first account indices.
    cached: WalletSummary,
    /// The address most recently copied from this row.
    copied: Option<String>,
    message: String,
}

impl Default for AccountView {
    fn default() -> Self {
        Self {
            wallet: String::new(),
            curve: Curve::Secp256k1,
            full_path: false,
            index: "0".into(),
            path: DEFAULT_ETH_PATH.into(),
            address: None,
            cached: WalletSummary {
                name: String::new(),
                ethereum_address: None,
                solana_address: None,
                ethereum_accounts: Vec::new(),
                solana_accounts: Vec::new(),
                saved_accounts: Vec::new(),
            },
            copied: None,
            message: String::new(),
        }
    }
}

impl AccountView {
    pub(super) fn select_wallet(&mut self, name: &str) {
        if self.wallet != name {
            *self = Self {
                wallet: name.into(),
                ..Self::default()
            };
        }
    }

    /// Adopts the wallet's current display cache. An address derived earlier belongs to
    /// the previous cache, so it is dropped when the cache changes (e.g. after Sync from
    /// Keychain); a fresh derivation is remembered in the new cache and still shows.
    pub(super) fn set_cached(&mut self, entry: &WalletSummary) {
        if self.cached != *entry {
            self.cached = entry.clone();
            self.address = None;
        }
    }

    /// The address for the current inputs without reading the key: the one derived
    /// after authentication, or a cached address for the first account indices.
    fn known_address(&self) -> Option<String> {
        self.address.clone().or_else(|| {
            let request = self.request().ok()?;
            self.cached
                .cached_account(request.curve, &request.path)
                .map(str::to_owned)
        })
    }

    fn request(&self) -> Result<KeyRequest> {
        let path = if self.full_path {
            self.path.clone()
        } else {
            if self.index.is_empty() || !self.index.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(Failure::Input);
            }
            let index = self.index.parse::<u32>().map_err(|_| Failure::Input)?;
            if index >= 0x8000_0000 {
                return Err(Failure::Input);
            }
            crate::model::account_path(self.curve, index)
        };
        let request = KeyRequest {
            name: self.wallet.clone(),
            curve: self.curve,
            path,
        };
        request.validate()?;
        Ok(request)
    }

    /// Draws the network, path mode and index/path rows.
    fn inputs(&mut self, ui: &mut egui::Ui) {
        let previous = (
            self.curve,
            self.full_path,
            self.index.clone(),
            self.path.clone(),
        );
        theme::form_row(ui, "Network", |ui| curve_selector(ui, &mut self.curve));
        if self.curve != previous.0 {
            self.path = match self.curve {
                Curve::Secp256k1 => DEFAULT_ETH_PATH,
                Curve::Ed25519 => crate::model::DEFAULT_SOL_PATH,
            }
            .into();
        }
        theme::group_separator(ui);
        theme::form_row(ui, "Custom path", |ui| {
            ui.checkbox(&mut self.full_path, "")
                .on_hover_text("Enter a full derivation path instead of an account index")
        });
        theme::group_separator(ui);
        let full_path = self.full_path;
        let resolved = self.request().ok().map(|request| request.path);
        let (label, value, limit, width) = if full_path {
            let label = if self.curve == Curve::Ed25519 {
                "Hardened path"
            } else {
                "Path"
            };
            (label, &mut self.path, 128, f32::INFINITY)
        } else {
            ("Account index", &mut self.index, 10, 96.0)
        };
        theme::form_row(ui, label, |ui| {
            let field = theme::text_field(ui, value, limit, width);
            if !full_path {
                ui.add_space(4.0);
                // Shown even before editing so the index's meaning is clear; reserves
                // its line when invalid so the action below never moves.
                let text = resolved.as_deref().unwrap_or("");
                ui.add(
                    egui::Label::new(egui::RichText::new(text).monospace().color(theme::MUTED))
                        .truncate(),
                );
            }
            field
        });
        if previous
            != (
                self.curve,
                self.full_path,
                self.index.clone(),
                self.path.clone(),
            )
        {
            self.address = None;
            self.message.clear();
        }
        theme::group_separator(ui);
    }

    /// The derived address row. Before authentication it offers to copy; a click
    /// authenticates, derives and copies in one step. Afterwards it shows the
    /// abbreviated address, or the full one while the copy is on the clipboard.
    fn address_row(&mut self, ui: &mut egui::Ui, copied: bool) -> Option<AddressAction> {
        let focused = ui.ctx().input(|input| input.focused);
        let known = self.known_address();
        // Full while this exact address is on the clipboard, abbreviated otherwise.
        let copied = copied && known.is_some() && self.copied == known;
        let row = match &known {
            Some(address) => {
                let shown = if copied {
                    address.clone()
                } else {
                    theme::abbreviate_address(address)
                };
                theme::field_row(
                    ui,
                    "Address",
                    theme::RowValue::Mono(&shown),
                    Some(if copied { "Copied" } else { "Copy" }),
                )
            }
            None => {
                ui.add_enabled_ui(focused, |ui| {
                    theme::field_row(
                        ui,
                        "Address",
                        // Unknown until the click authenticates; it is then remembered.
                        theme::RowValue::Mono(""),
                        Some("Copy"),
                    )
                })
                .inner
            }
        };
        if !row.clicked() {
            return None;
        }
        self.message.clear();
        if let Some(address) = known {
            return Some(AddressAction::Copy(address));
        }
        if let Ok(request) = self.request() {
            Some(AddressAction::Derive(request))
        } else {
            self.message = if self.full_path {
                "Enter a valid derivation path. Solana requires every component to be hardened."
            } else {
                "Enter a whole-number index from 0 to 2147483647."
            }
            .into();
            None
        }
    }
}

enum AddressAction {
    Copy(String),
    Derive(KeyRequest),
}

impl WalletApp {
    fn remember_summary(&mut self, summary: WalletSummary) {
        let stored = Catalog::for_group(&self.vault.group)
            .and_then(|catalog| catalog.upsert(summary.clone()));
        if stored.is_err() {
            // Appended so a copy failure reported earlier is never masked.
            let message = &mut self.account_view.message;
            if !message.is_empty() {
                message.push(' ');
            }
            message.push_str("The address could not be saved for next time.");
        }
        self.account_view.set_cached(&summary);
        if let Some(entry) = self.wallets.iter_mut().find(|e| e.name == summary.name) {
            *entry = summary;
        }
    }

    fn copy_derived_address(&mut self, address: &str) {
        match self.clipboard.copy(address) {
            Ok(()) => {
                self.copied = Some("Address");
                self.account_view.copied = Some(address.to_owned());
            }
            Err(_) => self.account_view.message = "Unable to copy the address.".into(),
        }
    }

    pub(super) fn derived_account(&mut self, ui: &mut egui::Ui) {
        let copied = self.copied == Some("Address") && self.clipboard.has_copy();
        let action = theme::group(ui, |ui| {
            self.account_view.inputs(ui);
            self.account_view.address_row(ui, copied)
        })
        .inner;
        match action {
            Some(AddressAction::Copy(address)) => self.copy_derived_address(&address),
            Some(AddressAction::Derive(request)) => {
                self.prepare_authentication();
                let reason = format!(
                    "copy a derived address for wallet \"{}\"\nCurve: {}\nPath: {}",
                    request.name,
                    request.curve.name(),
                    request.path
                );
                let context = macos::AuthContext::new(&reason, false);
                // Refresh the wallet's cached accounts from the same read, so wallets
                // saved before caching fill indices 0–9, remembered paths are re-derived,
                // and this address is remembered. One seed serves every derivation.
                let previous = self.account_view.cached.saved_accounts.clone();
                match self.vault.read(&request.name, &context).and_then(|wallet| {
                    let seed = WalletSeed::new(&wallet)?;
                    let address = seed.derive(&request, None)?.address;
                    let mut summary = WalletSummary::from_seed(&request.name, &seed, &previous)?;
                    summary.remember(request.curve, &request.path, &address);
                    Ok((address, summary))
                }) {
                    Ok((address, summary)) => {
                        self.account_view.address = Some(address.clone());
                        self.copy_derived_address(&address);
                        self.remember_summary(summary);
                    }
                    Err(error) => {
                        self.account_view.message =
                            format!("Address not copied ({}).", error.code());
                    }
                }
            }
            None => {}
        }
        if !self.account_view.message.is_empty() {
            theme::footnote(ui, &self.account_view.message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StoredWallet, derive};

    #[test]
    fn editing_account_index_keeps_the_address_row_in_place() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut view = AccountView::default();
        view.select_wallet("public-test");
        let mut positions = Vec::new();
        for index in ["0", "", "2147483648", "2147483647", "2"] {
            view.index = index.into();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420.0, 300.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        view.inputs(ui);
                        assert!(view.address_row(ui, false).is_none());
                    });
                },
            );
            let row_y = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Address" => Some(text.pos.y),
                _ => None,
            });
            positions.push(row_y.expect("address row should be visible"));
        }
        assert!(
            positions.iter().all(|y| (y - positions[0]).abs() < 0.5),
            "{positions:?}"
        );
    }

    #[test]
    fn form_captions_share_one_column_at_every_width() {
        for width in [400.0, 560.0] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut view = AccountView::default();
            view.select_wallet("public-test");
            let mut bounds = egui::Rect::NOTHING;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        bounds = ui.max_rect();
                        let card = theme::group(ui, |ui| {
                            view.inputs(ui);
                            assert!(view.address_row(ui, false).is_none());
                        });
                        assert!(bounds.contains_rect(card.response.rect), "{width}");
                    });
                },
            );
            let left = |label: &str| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos.x),
                    _ => None,
                })
            };
            let network = left("Network").expect("caption");
            for label in ["Custom path", "Account index", "Address"] {
                let x = left(label).unwrap_or_else(|| panic!("{label} missing"));
                assert!(
                    (x - network).abs() < 1.0,
                    "{label} at {x}, Network at {network}"
                );
            }
        }
    }

    #[test]
    fn address_row_keeps_its_height_once_derived() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut view = AccountView::default();
        view.select_wallet("public-test");
        let mut heights = Vec::new();
        for (address, copied) in [
            (None, false),
            (Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"), false),
            (Some("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"), true),
        ] {
            view.address = address.map(Into::into);
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let card = theme::group(ui, |ui| {
                            view.inputs(ui);
                            assert!(view.address_row(ui, copied).is_none());
                        });
                        heights.push(card.response.rect.height());
                    });
                },
            );
        }
        assert!(
            heights.iter().all(|h| (h - heights[0]).abs() < 1.0),
            "{heights:?}"
        );
    }

    #[test]
    fn cached_accounts_show_without_authentication_and_fall_back_to_the_mask() {
        let wallet = StoredWallet {
            version: 1,
            mnemonic: "test test test test test test test test test test test junk".into(),
            passphrase: String::new(),
        };
        let summary = WalletSummary::from_wallet("public-test", &wallet, &[]).unwrap();
        let mut view = AccountView::default();
        view.select_wallet("public-test");
        view.set_cached(&summary);
        view.index = "2".into();
        assert_eq!(
            view.known_address().as_deref(),
            Some("0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc")
        );
        view.curve = Curve::Ed25519;
        view.index = "0".into();
        assert_eq!(
            view.known_address().as_deref(),
            Some("oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96")
        );
        // Beyond the cached range or off the standard path, the key must be read.
        view.index = (crate::model::CACHED_ACCOUNTS).to_string();
        assert!(view.known_address().is_none());
        view.full_path = true;
        view.path = "m/44'/501'/0'/1'".into();
        assert!(view.known_address().is_none());
        // Wallets without any cached address stay unknown.
        view.set_cached(&WalletSummary::named("public-test".into()));
        view.full_path = false;
        view.index = "0".into();
        assert!(view.known_address().is_none());
        // Wallets saved before the index cache still know index 0 from their defaults.
        let mut legacy = summary.clone();
        legacy.ethereum_accounts.clear();
        legacy.solana_accounts.clear();
        view.set_cached(&legacy);
        view.curve = Curve::Secp256k1;
        assert_eq!(
            view.known_address().as_deref(),
            legacy.ethereum_address.as_deref()
        );
        view.curve = Curve::Ed25519;
        assert_eq!(
            view.known_address().as_deref(),
            legacy.solana_address.as_deref()
        );
        view.index = "1".into();
        assert!(view.known_address().is_none());
    }

    #[test]
    fn account_indices_and_custom_paths_use_the_selected_curve() {
        let mut view = AccountView::default();
        view.select_wallet("public-test");
        for (curve, path) in [
            (Curve::Secp256k1, "m/44'/60'/0'/0/2"),
            (Curve::Ed25519, "m/44'/501'/2'/0'"),
        ] {
            view.curve = curve;
            view.index = "2".into();
            assert_eq!(view.request().unwrap().path, path);
            view.index = "2147483647".into();
            assert!(view.request().is_ok());
            for invalid in ["", "-1", "+1", "1.5", " 2", "2147483648", "4294967296"] {
                view.index = invalid.into();
                assert!(view.request().is_err());
            }
            view.full_path = true;
            view.path = path.into();
            assert_eq!(view.request().unwrap().path, path);
            view.path = "m/44'/501'/2'/0".into();
            assert!(view.request().is_err());
            view.full_path = false;
        }
    }

    #[test]
    fn a_changed_cache_replaces_the_derived_address() {
        let stored = |mnemonic: &str| StoredWallet {
            version: 1,
            mnemonic: mnemonic.into(),
            passphrase: String::new(),
        };
        let old = stored("test test test test test test test test test test test junk");
        let new = stored(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        );
        let mut view = AccountView::default();
        view.select_wallet("public-test");
        view.index = "12".into();
        let request = view.request().unwrap();
        let old_address = derive(&old, &request, None).unwrap().address;
        let mut summary = WalletSummary::from_wallet("public-test", &old, &[]).unwrap();
        summary.remember(request.curve, &request.path, &old_address);
        view.set_cached(&summary);
        view.address = Some(old_address.clone());
        // The wallet is replaced under the same name and Sync re-derives the cache.
        let synced =
            WalletSummary::from_wallet("public-test", &new, &summary.saved_accounts).unwrap();
        view.set_cached(&synced);
        let new_address = derive(&new, &request, None).unwrap().address;
        assert_ne!(new_address, old_address);
        assert_eq!(view.known_address(), Some(new_address));
    }

    #[test]
    fn switching_wallets_clears_the_previous_address_and_inputs() {
        let mut view = AccountView::default();
        view.select_wallet("first");
        view.index = "2".into();
        view.address = Some("previous address".into());
        view.message = "Address copied.".into();
        view.select_wallet("second");
        assert_eq!(view.request().unwrap().name, "second");
        assert_eq!(view.index, "0");
        assert!(view.address.is_none());
        assert!(view.message.is_empty());
    }

    #[test]
    fn displayed_addresses_match_independent_public_vectors() {
        let wallet = StoredWallet {
            version: 1,
            mnemonic: "test test test test test test test test test test test junk".into(),
            passphrase: String::new(),
        };
        let mut view = AccountView::default();
        view.select_wallet("public-test");
        assert_eq!(
            derive(&wallet, &view.request().unwrap(), None)
                .unwrap()
                .address,
            "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"
        );
        view.index = "2".into();
        assert_eq!(
            derive(&wallet, &view.request().unwrap(), None)
                .unwrap()
                .address,
            "0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc"
        );
        view.curve = Curve::Ed25519;
        view.index = "0".into();
        assert_eq!(
            derive(&wallet, &view.request().unwrap(), None)
                .unwrap()
                .address,
            "oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96"
        );
    }
}
