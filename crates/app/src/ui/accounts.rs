use super::{curve_selector, WalletApp};
use crate::{
    macos,
    model::{derive, Curve, Failure, KeyRequest, Result, DEFAULT_ETH_PATH},
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
            message: String::new(),
        }
    }
}
impl AccountView {
    pub(super) fn select_wallet(&mut self, name: &str) {
        if self.wallet != name {
            *self = Self { wallet: name.into(), ..Self::default() };
        }
    }

    fn request(&self) -> Result<KeyRequest> {
        let path = if self.full_path {
            self.path.clone()
        } else {
            if self.index.is_empty() || !self.index.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(Failure::Input);
            }
            let index = self.index.parse::<u32>().map_err(|_| Failure::Input)?;
            if index >= 0x80000000 {
                return Err(Failure::Input);
            }
            match self.curve {
                Curve::Secp256k1 => format!("m/44'/60'/0'/0/{index}"),
                Curve::Ed25519 => format!("m/44'/501'/{index}'/0'"),
            }
        };
        let request = KeyRequest { name: self.wallet.clone(), curve: self.curve, path };
        request.validate()?;
        Ok(request)
    }

    fn inputs(&mut self, ui: &mut egui::Ui) -> Option<KeyRequest> {
        let previous = (self.curve, self.full_path, self.index.clone(), self.path.clone());
        curve_selector(ui, &mut self.curve);
        if self.curve != previous.0 {
            self.path = match self.curve {
                Curve::Secp256k1 => DEFAULT_ETH_PATH,
                Curve::Ed25519 => crate::model::DEFAULT_SOL_PATH,
            }.into();
        }
        ui.add_space(8.0);
        ui.checkbox(&mut self.full_path, "Use full derivation path");
        if self.full_path {
            theme::public_field(ui, "Derivation path", &mut self.path, 128);
        } else {
            theme::public_field(ui, "Account index", &mut self.index, 10);
        }
        if self.curve == Curve::Ed25519 {
            theme::hint(ui, "Solana uses m/44'/501'/index'/0'. Use the original wallet's full hardened path if different.");
        }
        let request = self.request();
        if let Ok(request) = &request {
            theme::label(ui, "Path");
            ui.add(egui::Label::new(egui::RichText::new(&request.path).monospace().size(12.0)).wrap());
        }
        if previous != (self.curve, self.full_path, self.index.clone(), self.path.clone()) {
            self.address = None;
            self.message.clear();
        }
        ui.add_space(8.0);
        theme::hint(ui, "Authenticate to derive this address. Only the public address is displayed.");
        let focused = ui.ctx().input(|input| input.focused);
        if ui.add_enabled_ui(focused, |ui| {
            theme::primary(ui, "Authenticate and view")
        }).inner.clicked() {
            self.address = None;
            self.message.clear();
            match request {
                Ok(request) => return Some(request),
                Err(_) => self.message = if self.full_path {
                    "Enter a valid derivation path. Solana requires every component to be hardened."
                } else {
                    "Enter a whole-number index from 0 to 2147483647."
                }.into(),
            }
        }
        None
    }
}

impl WalletApp {
    pub(super) fn derived_account(&mut self, ui: &mut egui::Ui) {
        if let Some(request) = self.account_view.inputs(ui) {
            self.prepare_authentication();
            let reason = format!("view a derived address for wallet \"{}\"\nCurve: {}\nPath: {}",
                request.name, request.curve.name(), request.path);
            let context = macos::AuthContext::new(&reason, false);
            match self.vault.read(&request.name, &context)
                .and_then(|wallet| derive(&wallet, &request, None).map(|pair| pair.address)) {
                Ok(address) => self.account_view.address = Some(address),
                Err(error) => self.account_view.message = format!("Address not shown ({}).", error.code()),
            }
        }
        if let Some(address) = &self.account_view.address {
            theme::account_row(ui, "Address", Some(address));
            if theme::secondary(ui, "Copy address").clicked() {
                self.account_view.message = match self.clipboard.copy(address) {
                    Ok(()) => "Address copied. Clipboard clears after 60 seconds while the app is open.".into(),
                    Err(_) => "Unable to copy the address.".into(),
                };
            }
        }
        if !self.account_view.message.is_empty() {
            theme::hint(ui, &self.account_view.message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::StoredWallet;

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
        assert_eq!(derive(&wallet, &view.request().unwrap(), None).unwrap().address,
            "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266");
        view.index = "2".into();
        assert_eq!(derive(&wallet, &view.request().unwrap(), None).unwrap().address,
            "0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc");
        view.curve = Curve::Ed25519;
        view.index = "0".into();
        assert_eq!(derive(&wallet, &view.request().unwrap(), None).unwrap().address,
            "oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96");
    }
}
