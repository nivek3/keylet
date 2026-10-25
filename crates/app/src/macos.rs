use crate::model::{Failure, Result, StoredWallet, WalletSummary};
use core_foundation::{
    array::CFArray,
    base::{CFType, TCFType},
    boolean::CFBoolean,
    data::CFData,
    dictionary::CFDictionary,
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use objc2::{rc::Retained, runtime::AnyObject, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask, NSEventModifierFlags,
    NSPasteboard,
};
use objc2_foundation::{NSBundle, NSString};
use objc2_local_authentication::LAContext;
use security_framework_sys::{
    access_control::*, code_signing::*, item::*, keychain_item::*, random::*,
};
use std::{
    ptr::{self, NonNull},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

type Dict = CFDictionary<CFString, CFType>;
#[link(name = "Security", kind = "framework")]
extern "C" {
    fn SecCodeCopyStaticCode(code: SecCodeRef, flags: u32, output: *mut SecStaticCodeRef) -> i32;
    fn SecCodeCopySigningInformation(
        code: SecStaticCodeRef,
        flags: u32,
        output: *mut core_foundation::dictionary::CFDictionaryRef,
    ) -> i32;
    static kSecCodeInfoEntitlementsDict: CFStringRef;
    static kSecCodeInfoFlags: CFStringRef;
    static kSecMatchLimitOne: CFStringRef;
}
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn EnableSecureEventInput() -> i32;
    fn DisableSecureEventInput() -> i32;
}
fn status(code: i32) -> Result<()> {
    match code {
        0 => Ok(()),
        -128 => Err(Failure::Cancelled),
        -25293 | -25308 => Err(Failure::Authentication),
        -25300 => Err(Failure::Missing),
        -25299 => Err(Failure::Duplicate),
        -34018 => Err(Failure::Configuration),
        _ => Err(Failure::Keychain),
    }
}
unsafe fn symbol(key: CFStringRef) -> CFString {
    CFString::wrap_under_get_rule(key)
}
fn value(dict: &Dict, key: &str) -> Option<CFType> {
    dict.find(&CFString::new(key)).map(|v| (*v).clone())
}
fn dictionary(value: CFType) -> Result<Dict> {
    let untyped = value
        .downcast::<CFDictionary>()
        .ok_or(Failure::Configuration)?;
    Ok(unsafe { Dict::wrap_under_get_rule(untyped.as_concrete_TypeRef()) })
}
fn string(value: Option<CFType>) -> Option<String> {
    value?.downcast::<CFString>().map(|s| s.to_string())
}
fn validate_signing(info: &Dict, bundle: &str, group: &str) -> Result<()> {
    if !group.ends_with(&format!(".{bundle}")) || group.contains('*') {
        return Err(Failure::Configuration);
    }
    let ent_key = unsafe { symbol(kSecCodeInfoEntitlementsDict) }.to_string();
    let flags_key = unsafe { symbol(kSecCodeInfoFlags) }.to_string();
    let ent = dictionary(value(info, &ent_key).ok_or(Failure::Configuration)?)?;
    let flags = value(info, &flags_key)
        .and_then(|v| v.downcast::<CFNumber>())
        .and_then(|n| n.to_i64())
        .ok_or(Failure::Configuration)?;
    let groups = value(&ent, "keychain-access-groups")
        .and_then(|v| v.downcast::<CFArray>())
        .ok_or(Failure::Configuration)?;
    let groups: Vec<_> = groups
        .get_all_values()
        .into_iter()
        .map(|v| unsafe { string(Some(CFType::wrap_under_get_rule(v))) })
        .collect();
    if groups != [Some(group.to_owned())]
        || string(value(&ent, "com.apple.application-identifier")).as_deref() != Some(group)
        || flags & 0x10000 == 0
        || flags & 0x2 != 0
    {
        return Err(Failure::Configuration);
    }
    for key in [
        "com.apple.security.get-task-allow",
        "get-task-allow",
        "com.apple.security.cs.disable-library-validation",
        "com.apple.security.cs.allow-dyld-environment-variables",
        "com.apple.security.cs.allow-unsigned-executable-memory",
    ] {
        if value(&ent, key).is_some_and(|v| v == CFBoolean::true_value().as_CFType()) {
            return Err(Failure::Configuration);
        }
    }
    Ok(())
}

pub struct AuthContext(Retained<LAContext>);
impl AuthContext {
    pub fn new(reason: &str, no_interaction: bool) -> Self {
        unsafe {
            let context = LAContext::new();
            context.setTouchIDAuthenticationAllowableReuseDuration(0.0);
            context.setLocalizedReason(&NSString::from_str(reason));
            context.setLocalizedCancelTitle(Some(&NSString::from_str("Cancel")));
            context.setInteractionNotAllowed(no_interaction);
            Self(context)
        }
    }
    fn as_cf(&self) -> CFType {
        unsafe { CFType::wrap_under_get_rule(Retained::as_ptr(&self.0).cast()) }
    }
}
impl Drop for AuthContext {
    fn drop(&mut self) {
        unsafe {
            self.0.invalidate();
        }
    }
}

// Only the pipe-based Node helper retains this context; UI operations use fresh contexts.
#[derive(Default)]
pub struct AuthenticationSession {
    context: Option<AuthContext>,
    authenticated_at: Option<Instant>,
    authenticated_wall: Option<std::time::SystemTime>,
    ttl: Duration,
}
impl AuthenticationSession {
    pub fn clear(&mut self) {
        self.context = None;
        self.authenticated_at = None;
        self.authenticated_wall = None;
    }
    pub fn expire(&mut self, now: Instant) {
        if self.authenticated_at.is_some_and(|start| now.duration_since(start) >= self.ttl)
            || self.authenticated_wall.is_some_and(|start| start.elapsed().map_or(true, |age| age >= self.ttl))
        {
            self.clear();
        }
    }
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        self.authenticated_at.map(|start| self.ttl.saturating_sub(now.duration_since(start)))
    }
    pub fn context(&mut self, reason: &str, ttl: Duration, now: Instant) -> &AuthContext {
        self.expire(now);
        if self.ttl != ttl {
            self.clear();
            self.ttl = ttl;
        }
        self.context.get_or_insert_with(|| AuthContext::new(reason, false))
    }
    pub fn complete(&mut self, success: bool, now: Instant) {
        if success && !self.ttl.is_zero() {
            self.authenticated_at.get_or_insert(now);
            self.authenticated_wall.get_or_insert_with(std::time::SystemTime::now);
        } else {
            self.clear();
        }
    }
}

pub struct Vault {
    pub group: String,
}
impl Vault {
    pub fn new() -> Result<Self> {
        let bundle = NSBundle::mainBundle();
        let id = bundle
            .bundleIdentifier()
            .ok_or(Failure::Configuration)?
            .to_string();
        let group = bundle
            .objectForInfoDictionaryKey(&NSString::from_str("WalletAccessGroup"))
            .and_then(|v| v.downcast::<NSString>().ok())
            .ok_or(Failure::Configuration)?
            .to_string();
        if !std::path::Path::new(&bundle.bundlePath().to_string())
            .join("Contents/embedded.provisionprofile")
            .is_file()
        {
            return Err(Failure::Configuration);
        }
        unsafe {
            let mut code = ptr::null_mut();
            if SecCodeCopySelf(0, &mut code) != 0 || code.is_null() {
                return Err(Failure::Configuration);
            }
            let _code = CFType::wrap_under_create_rule(code.cast());
            if SecCodeCheckValidity(code, 0, ptr::null_mut()) != 0 {
                return Err(Failure::Configuration);
            }
            let mut static_code = ptr::null_mut();
            if SecCodeCopyStaticCode(code, 0, &mut static_code) != 0 || static_code.is_null() {
                return Err(Failure::Configuration);
            }
            let _static = CFType::wrap_under_create_rule(static_code.cast());
            let mut info = ptr::null();
            if SecCodeCopySigningInformation(static_code, 1 << 1, &mut info) != 0 || info.is_null()
            {
                return Err(Failure::Configuration);
            }
            let info = Dict::wrap_under_create_rule(info);
            validate_signing(&info, &id, &group)?;
        }
        Ok(Self { group })
    }
    fn query(&self, name: Option<&str>) -> Vec<(CFString, CFType)> {
        unsafe {
            let mut pairs = vec![
                (
                    symbol(kSecClass),
                    symbol(kSecClassGenericPassword).as_CFType(),
                ),
                (
                    symbol(kSecUseDataProtectionKeychain),
                    CFBoolean::true_value().as_CFType(),
                ),
                (
                    symbol(kSecAttrAccessGroup),
                    CFString::new(&self.group).as_CFType(),
                ),
                (
                    symbol(kSecAttrService),
                    CFString::new("local.keychain-wallet.mnemonic.v1").as_CFType(),
                ),
                (
                    symbol(kSecAttrSynchronizable),
                    CFBoolean::false_value().as_CFType(),
                ),
            ];
            if let Some(name) = name {
                pairs.push((symbol(kSecAttrAccount), CFString::new(name).as_CFType()));
            }
            pairs
        }
    }
    pub fn insert(&self, name: &str, wallet: &StoredWallet) -> Result<()> {
        crate::model::validate_name(name)?;
        let data = Zeroizing::new(serde_json::to_vec(wallet).map_err(|_| Failure::Corrupt)?);
        if wallet.version != 1 || data.len() > 16384 {
            return Err(Failure::Corrupt);
        }
        unsafe {
            let mut error = ptr::null_mut();
            let acl = SecAccessControlCreateWithFlags(
                ptr::null(),
                kSecAttrAccessibleWhenUnlockedThisDeviceOnly.cast(),
                kSecAccessControlUserPresence,
                &mut error,
            );
            if !error.is_null() {
                let _ = CFType::wrap_under_create_rule(error.cast());
            }
            if acl.is_null() {
                return Err(Failure::Configuration);
            }
            let acl = CFType::wrap_under_create_rule(acl.cast());
            let mut q = self.query(Some(name));
            q.push((symbol(kSecAttrAccessControl), acl));
            q.push((
                symbol(kSecAttrLabel),
                CFString::new(&format!("Keylet: {name}")).as_CFType(),
            ));
            q.push((
                symbol(kSecValueData),
                CFData::from_buffer(&data).as_CFType(),
            ));
            // Never update duplicates or fall back to the file-based Keychain.
            status(SecItemAdd(
                Dict::from_CFType_pairs(&q).as_concrete_TypeRef(),
                ptr::null_mut(),
            ))
        }
    }
    pub fn read(&self, name: &str, context: &AuthContext) -> Result<StoredWallet> {
        crate::model::validate_name(name)?;
        unsafe {
            let mut q = self.query(Some(name));
            q.push((symbol(kSecUseAuthenticationContext), context.as_cf()));
            q.push((symbol(kSecReturnData), CFBoolean::true_value().as_CFType()));
            q.push((
                symbol(kSecMatchLimit),
                symbol(kSecMatchLimitOne).as_CFType(),
            ));
            let mut result = ptr::null();
            status(SecItemCopyMatching(
                Dict::from_CFType_pairs(&q).as_concrete_TypeRef(),
                &mut result,
            ))?;
            if result.is_null() {
                return Err(Failure::Corrupt);
            }
            let item = CFType::wrap_under_create_rule(result);
            let data = item.downcast::<CFData>().ok_or(Failure::Corrupt)?;
            if data.len() > 16384 {
                return Err(Failure::Corrupt);
            }
            let owned = Zeroizing::new(data.bytes().to_vec());
            let wallet: StoredWallet =
                serde_json::from_slice(&owned).map_err(|_| Failure::Corrupt)?;
            if wallet.version != 1 {
                return Err(Failure::Corrupt);
            }
            Ok(wallet)
        }
    }
    fn names_query(&self, context: &AuthContext) -> Dict {
        unsafe {
            let mut q = self.query(None);
            q.push((symbol(kSecUseAuthenticationContext), context.as_cf()));
            q.push((
                symbol(kSecReturnAttributes),
                CFBoolean::true_value().as_CFType(),
            ));
            q.push((
                symbol(kSecMatchLimit),
                symbol(kSecMatchLimitAll).as_CFType(),
            ));
            Dict::from_CFType_pairs(&q)
        }
    }
    fn names_with_context(&self, context: &AuthContext) -> Result<Vec<String>> {
        unsafe {
            let q = self.names_query(context);
            let mut output = ptr::null();
            let code = SecItemCopyMatching(q.as_concrete_TypeRef(), &mut output);
            if code == -25300 {
                return Ok(Vec::new());
            }
            status(code)?;
            if output.is_null() {
                return Err(Failure::Corrupt);
            }
            let rows = CFType::wrap_under_create_rule(output)
                .downcast::<CFArray>()
                .ok_or(Failure::Corrupt)?;
            let mut names = Vec::new();
            for row in rows.get_all_values() {
                let row =
                    dictionary(CFType::wrap_under_get_rule(row)).map_err(|_| Failure::Corrupt)?;
                if let Some(name) = string(value(&row, &symbol(kSecAttrAccount).to_string())) {
                    names.push(name);
                }
            }
            names.sort();
            Ok(names)
        }
    }
    pub fn summaries(&self) -> Result<Vec<WalletSummary>> {
        let context = AuthContext::new(
            "Sync wallet names and default ETH / Solana addresses",
            false,
        );
        self.names_with_context(&context)?
            .into_iter()
            .map(|name| {
                let wallet = self.read(&name, &context)?;
                WalletSummary::from_wallet(&name, &wallet)
            })
            .collect()
    }
    pub fn delete(&self, name: &str, context: &AuthContext) -> Result<()> {
        crate::model::validate_name(name)?;
        unsafe {
            let mut q = self.query(Some(name));
            q.push((symbol(kSecUseAuthenticationContext), context.as_cf()));
            status(SecItemDelete(
                Dict::from_CFType_pairs(&q).as_concrete_TypeRef(),
            ))
        }
    }
    pub fn delete_authenticated(&self, name: &str) -> Result<()> {
        let context = AuthContext::new(&format!("Delete wallet {name}"), false);
        self.read(name, &context)?;
        self.delete(name, &context)
    }
}

pub fn initialize_accessory() -> Result<()> {
    let mtm = MainThreadMarker::new().ok_or(Failure::Configuration)?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();
    Ok(())
}
pub fn generate_mnemonic() -> Result<Zeroizing<String>> {
    let mut entropy = Zeroizing::new([0u8; 16]);
    if unsafe { SecRandomCopyBytes(kSecRandomDefault, 16, entropy.as_mut_ptr().cast()) } != 0 {
        return Err(Failure::Derivation);
    }
    wallet_crypto::mnemonic_from_entropy(&entropy).map_err(|_| Failure::Derivation)
}

pub struct SecureInput(bool);
impl SecureInput {
    pub fn new() -> Self {
        Self(false)
    }
    pub fn set(&mut self, enabled: bool) -> Result<()> {
        if self.0 != enabled {
            if unsafe {
                if enabled {
                    EnableSecureEventInput()
                } else {
                    DisableSecureEventInput()
                }
            } != 0
            {
                return Err(Failure::Configuration);
            }
            self.0 = enabled;
        }
        Ok(())
    }
}
impl Drop for SecureInput {
    fn drop(&mut self) {
        let _ = self.set(false);
    }
}

pub struct Clipboard {
    board: Retained<NSPasteboard>,
    expiry: Option<(Instant, isize)>,
}
impl Clipboard {
    pub fn new() -> Self {
        Self {
            board: NSPasteboard::generalPasteboard(),
            expiry: None,
        }
    }
    pub fn copy(&mut self, words: &str) -> Result<()> {
        self.board.clearContents();
        if !self.board.setString_forType(
            &NSString::from_str(words),
            &NSString::from_str("public.utf8-plain-text"),
        ) {
            return Err(Failure::Transport);
        }
        self.expiry = Some((
            Instant::now() + Duration::from_secs(60),
            self.board.changeCount(),
        ));
        Ok(())
    }
    #[cfg(test)]
    fn paste_confirmation(&self) -> Result<Zeroizing<String>> {
        let value = self
            .board
            .stringForType(&NSString::from_str("public.utf8-plain-text"))
            .ok_or(Failure::Input)?;
        if value.length() > 4096 {
            return Err(Failure::Input);
        }
        let value = Zeroizing::new(value.to_string());
        if value.len() > 4096 {
            return Err(Failure::Input);
        }
        Ok(value)
    }
    pub fn tick(&mut self, now: Instant) {
        if let Some((deadline, count)) = self.expiry {
            if now >= deadline {
                self.clear();
            } else if self.board.changeCount() != count {
                self.expiry = None;
            }
        }
    }
    pub fn has_copy(&self) -> bool {
        self.expiry.is_some_and(|(_, count)| self.board.changeCount() == count)
    }
    pub fn clear(&mut self) {
        if let Some((_, count)) = self.expiry.take() {
            if self.board.changeCount() == count {
                self.board.clearContents();
            }
        }
    }
}
// Intercept before winit can read the system clipboard; egui's input hook is too late.
pub struct ClipboardGate {
    monitor: Retained<AnyObject>,
}
impl ClipboardGate {
    pub fn new() -> Result<Self> {
        let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let e = unsafe { event.as_ref() };
            if e.modifierFlags().contains(NSEventModifierFlags::Command) {
                let key = e
                    .charactersIgnoringModifiers()
                    .map(|s| s.to_string().to_ascii_lowercase())
                    .unwrap_or_default();
                if matches!(key.as_str(), "c" | "x" | "v") {
                    return ptr::null_mut();
                }
            }
            event.as_ptr()
        });
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
        }
        .ok_or(Failure::Configuration)?;
        Ok(Self { monitor })
    }
}
impl Drop for ClipboardGate {
    fn drop(&mut self) {
        unsafe {
            NSEvent::removeMonitor(&self.monitor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_authentication_window_is_fixed_and_zero_never_retains_a_context() {
        let now = Instant::now();
        let mut session = AuthenticationSession::default();
        session.context("Public fixture", Duration::from_secs(300), now);
        assert!(session.authenticated_at.is_none());
        session.complete(true, now);
        session.context("Public fixture", Duration::from_secs(300), now + Duration::from_secs(299));
        session.complete(true, now + Duration::from_secs(299));
        assert_eq!(session.remaining(now + Duration::from_secs(299)), Some(Duration::from_secs(1)));
        session.expire(now + Duration::from_secs(300));
        assert!(session.context.is_none());
        session.context("Public fixture", Duration::ZERO, now);
        session.complete(true, now);
        assert!(session.context.is_none());
        session.context("Public fixture", Duration::from_secs(300), now);
        session.complete(false, now);
        assert!(session.context.is_none());
        session.context("Public fixture", Duration::from_secs(300), now);
        session.complete(true, now);
        session.context("Public fixture", Duration::ZERO, now);
        assert!(session.authenticated_at.is_none());
        session.context("Public fixture", Duration::from_secs(300), now);
        session.complete(true, now);
        session.authenticated_wall = Some(std::time::SystemTime::now() - Duration::from_secs(301));
        session.expire(now);
        assert!(session.context.is_none());
        session.context("Public fixture", Duration::from_secs(300), now);
        session.complete(true, now);
        session.authenticated_wall = Some(std::time::SystemTime::now() + Duration::from_secs(10));
        session.expire(now);
        assert!(session.context.is_none());
    }
    #[test]
    fn system_entropy_generates_a_valid_twelve_word_wallet_without_persistence() {
        let phrase = generate_mnemonic().unwrap();
        assert!(phrase.split_whitespace().count() == 12);
        let wallet = StoredWallet {
            version: 1,
            mnemonic: phrase.to_string(),
            passphrase: String::new(),
        };
        let request = crate::model::KeyRequest {
            name: "memory-only-test".into(),
            curve: crate::model::Curve::Secp256k1,
            path: "m/44'/60'/0'/0/0".into(),
        };
        assert!(crate::model::derive(&wallet, &request, None).is_ok());
    }
    #[test]
    fn clipboard_expiry_preserves_new_content_and_resets_timer() {
        let board = NSPasteboard::pasteboardWithUniqueName();
        let mut clipboard = Clipboard {
            board: board.clone(),
            expiry: None,
        };
        assert!(!clipboard.has_copy());
        board.setString_forType(
            &NSString::from_str("unrelated initial content"),
            &NSString::from_str("public.utf8-plain-text"),
        );
        assert!(!clipboard.has_copy());
        clipboard.copy("public clipboard test sentinel").unwrap();
        assert!(clipboard.has_copy());
        assert_eq!(
            clipboard.paste_confirmation().unwrap().as_str(),
            "public clipboard test sentinel"
        );
        clipboard.tick(Instant::now() + Duration::from_secs(61));
        assert!(clipboard.paste_confirmation().is_err());
        assert!(!clipboard.has_copy());
        clipboard.copy("public clipboard test sentinel").unwrap();
        board.clearContents();
        board.setString_forType(
            &NSString::from_str("new unrelated content"),
            &NSString::from_str("public.utf8-plain-text"),
        );
        clipboard.tick(Instant::now() + Duration::from_secs(61));
        assert_eq!(
            clipboard.paste_confirmation().unwrap().as_str(),
            "new unrelated content"
        );
        clipboard.copy("public test again").unwrap();
        let previous = clipboard.expiry.unwrap().0;
        clipboard.copy("public test again").unwrap();
        assert!(clipboard.expiry.unwrap().0 >= previous);
        clipboard.clear();
        assert!(!clipboard.has_copy());
        assert!(clipboard.paste_confirmation().is_err());
        clipboard.copy("public manual clear fixture").unwrap();
        board.clearContents();
        board.setString_forType(
            &NSString::from_str("new unrelated content"),
            &NSString::from_str("public.utf8-plain-text"),
        );
        assert!(!clipboard.has_copy());
        clipboard.clear();
        assert_eq!(clipboard.paste_confirmation().unwrap().as_str(), "new unrelated content");
        unsafe {
            let _: () = objc2::msg_send![&*board, releaseGlobally];
        }
    }
    #[test]
    fn private_data_protection_query_and_fresh_authentication_context() {
        let vault = Vault {
            group: "PREFIX.com.example.Wallet".into(),
        };
        let query = Dict::from_CFType_pairs(&vault.query(Some("public-test")));
        unsafe {
            assert_eq!(
                value(&query, &symbol(kSecUseDataProtectionKeychain).to_string()),
                Some(CFBoolean::true_value().as_CFType())
            );
            assert_eq!(
                value(&query, &symbol(kSecAttrSynchronizable).to_string()),
                Some(CFBoolean::false_value().as_CFType())
            );
            assert_eq!(
                string(value(&query, &symbol(kSecAttrService).to_string())).as_deref(),
                Some("local.keychain-wallet.mnemonic.v1")
            );
            assert_eq!(
                string(value(&query, &symbol(kSecAttrAccessGroup).to_string())).as_deref(),
                Some(vault.group.as_str())
            );
            let context = AuthContext::new("Public configuration test", true);
            assert_eq!(context.0.touchIDAuthenticationAllowableReuseDuration(), 0.0);
            assert!(context.0.interactionNotAllowed());
            let interactive = AuthContext::new("List saved wallet names", false);
            assert!(!interactive.0.interactionNotAllowed());
            let names = vault.names_query(&interactive);
            assert_eq!(
                value(&names, &symbol(kSecReturnAttributes).to_string()),
                Some(CFBoolean::true_value().as_CFType())
            );
            assert!(value(&names, &symbol(kSecReturnData).to_string()).is_none());
            assert!(value(&names, &symbol(kSecAttrAccount).to_string()).is_none());
        }
    }
    #[test]
    fn status_errors_do_not_expose_framework_messages() {
        assert_eq!(status(-34018), Err(Failure::Configuration));
        assert_eq!(status(-25293), Err(Failure::Authentication));
        assert_eq!(status(-25308), Err(Failure::Authentication));
        assert_eq!(status(-25299), Err(Failure::Duplicate));
        assert_eq!(status(-1), Err(Failure::Keychain));
    }
    #[test]
    fn signing_requires_hardened_private_group_without_debug_exceptions() {
        let group = "PREFIX.com.example.Wallet";
        let dict = |groups: &[&str], flags: i64, debug: bool| {
            let groups: Vec<_> = groups
                .iter()
                .map(|s| CFString::new(s).as_CFType())
                .collect();
            let ent = Dict::from_CFType_pairs(&[
                (
                    CFString::new("keychain-access-groups"),
                    CFArray::from_CFTypes(&groups).as_CFType(),
                ),
                (
                    CFString::new("com.apple.application-identifier"),
                    CFString::new(group).as_CFType(),
                ),
                (
                    CFString::new("get-task-allow"),
                    CFBoolean::from(debug).as_CFType(),
                ),
            ]);
            unsafe {
                Dict::from_CFType_pairs(&[
                    (symbol(kSecCodeInfoEntitlementsDict), ent.as_CFType()),
                    (symbol(kSecCodeInfoFlags), CFNumber::from(flags).as_CFType()),
                ])
            }
        };
        assert!(
            validate_signing(&dict(&[group], 0x10000, false), "com.example.Wallet", group).is_ok()
        );
        for info in [
            dict(&[group], 0, false),
            dict(&[group], 0x10002, false),
            dict(&[group, "PREFIX.other"], 0x10000, false),
            dict(&[group], 0x10000, true),
        ] {
            assert_eq!(
                validate_signing(&info, "com.example.Wallet", group),
                Err(Failure::Configuration)
            );
        }
    }
}
