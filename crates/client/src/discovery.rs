use super::{BridgeResult, ClientError};
use core_foundation::{
    array::{CFArray, CFArrayRef},
    base::TCFType,
    bundle::{CFBundle, CFBundleGetIdentifier},
    string::{CFString, CFStringRef},
    url::CFURL,
};
use std::{path::PathBuf, ptr};

const BUNDLE_ID: &str = "com.nivek3.keylet";

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn LSCopyApplicationURLsForBundleIdentifier(
        identifier: CFStringRef,
        error: *mut *const std::ffi::c_void,
    ) -> CFArrayRef;
}

fn bundle_executable(url: CFURL) -> BridgeResult<PathBuf> {
    let bundle = CFBundle::new(url).ok_or(ClientError::HelperStartFailed)?;
    // SAFETY: `bundle` is alive; the identifier is borrowed from it and may be null, which is checked next.
    let identifier = unsafe { CFBundleGetIdentifier(bundle.as_concrete_TypeRef()) };
    // SAFETY: `identifier` is non-null on this branch and owned by `bundle`; the get rule retains it.
    if identifier.is_null() || unsafe { CFString::wrap_under_get_rule(identifier) } != BUNDLE_ID {
        return Err(ClientError::HelperStartFailed);
    }
    bundle
        .executable_url()
        .and_then(|url| url.to_path())
        .ok_or(ClientError::HelperStartFailed)
}

pub(super) fn app_executable() -> BridgeResult<PathBuf> {
    let identifier = CFString::new(BUNDLE_ID);
    // SAFETY: `identifier` is a live CFString, and the error out-pointer is optional.
    let urls = unsafe {
        LSCopyApplicationURLsForBundleIdentifier(identifier.as_concrete_TypeRef(), ptr::null_mut())
    };
    if urls.is_null() {
        return Err(ClientError::AppNotFound);
    }
    // SAFETY: the copy returned an owned, non-null array, which the wrapper releases once.
    let urls: CFArray<CFURL> = unsafe { CFArray::wrap_under_create_rule(urls) };
    // macOS 10.15+ orders results with the preferred application first.
    let url = urls.get(0).ok_or(ClientError::AppNotFound)?;
    let selected = bundle_executable((*url).clone())?;
    // A broken preferred copy never falls back to another executable.
    selected
        .canonicalize()
        .map_err(|_| ClientError::HelperStartFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn renamed_bundles_use_metadata_and_reject_another_identifier() {
        for (name, identifier, valid) in [
            ("My Wallet.app", BUNDLE_ID, true),
            ("Keylet.app", "com.example.other", false),
        ] {
            let root = crate::tests::folder();
            let app = root.join(name);
            let executable = app.join("Contents/MacOS/custom-helper");
            fs::create_dir_all(executable.parent().unwrap()).unwrap();
            fs::write(&executable, "public fixture").unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
            fs::write(
                app.join("Contents/Info.plist"),
                format!(
                    "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict>\
                <key>CFBundleIdentifier</key><string>{identifier}</string>\
                <key>CFBundlePackageType</key><string>APPL</string>\
                <key>CFBundleExecutable</key><string>custom-helper</string>\
                </dict></plist>"
                ),
            )
            .unwrap();
            let result = bundle_executable(CFURL::from_path(&app, true).unwrap());
            if valid {
                assert_eq!(result.unwrap(), executable);
            } else {
                assert_eq!(result, Err(ClientError::HelperStartFailed));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
}
