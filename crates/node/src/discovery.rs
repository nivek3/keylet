use super::BridgeResult;
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
extern "C" {
    fn LSCopyApplicationURLsForBundleIdentifier(
        identifier: CFStringRef,
        error: *mut *const std::ffi::c_void,
    ) -> CFArrayRef;
}

fn bundle_executable(url: CFURL) -> BridgeResult<PathBuf> {
    let bundle = CFBundle::new(url).ok_or("Bundle not found")?;
    let identifier = unsafe { CFBundleGetIdentifier(bundle.as_concrete_TypeRef()) };
    if identifier.is_null()
        || unsafe { CFString::wrap_under_get_rule(identifier) }.to_string() != BUNDLE_ID
    {
        return Err("Bundle identifier mismatch");
    }
    bundle
        .executable_url()
        .and_then(|url| url.to_path())
        .ok_or("Executable not found")
}

#[allow(dead_code)]
pub(super) fn app_executable() -> BridgeResult<PathBuf> {
    let identifier = CFString::new(BUNDLE_ID);
    let urls = unsafe {
        LSCopyApplicationURLsForBundleIdentifier(identifier.as_concrete_TypeRef(), ptr::null_mut())
    };
    if urls.is_null() {
        return Err("KEYLET_APP_NOT_FOUND");
    }
    let urls: CFArray<CFURL> = unsafe { CFArray::wrap_under_create_rule(urls) };
    // macOS 10.15+ orders results with the preferred application first.
    let url = urls.get(0).ok_or("KEYLET_APP_NOT_FOUND")?;
    let selected = bundle_executable((*url).clone()).map_err(|_| "HELPER_START_FAILED")?;
    // A broken preferred copy never falls back to another executable.
    selected.canonicalize().map_err(|_| "HELPER_START_FAILED")
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
                assert_eq!(result, Err("Bundle identifier mismatch"));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
}
