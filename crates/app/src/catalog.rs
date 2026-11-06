use crate::model::{validate_name, Failure, Result, WalletSummary};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

// This is an untrusted display cache. Keychain remains authoritative for every key read and deletion.
pub struct Catalog {
    path: PathBuf,
}

#[derive(PartialEq, Eq)]
struct CatalogRevision {
    device: u64,
    inode: u64,
    changed_at: i64,
    changed_nanos: i64,
}

impl Catalog {
    pub fn for_group(group: &str) -> Result<Self> {
        if group.is_empty()
            || group.len() > 255
            || !group
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(Failure::Catalog);
        }
        let home = std::env::var_os("HOME").ok_or(Failure::Catalog)?;
        Ok(Self {
            path: PathBuf::from(home)
                .join("Library/Application Support/KeychainWallet")
                .join(group)
                .join("wallets.json"),
        })
    }
    fn validate(mut names: Vec<WalletSummary>) -> Result<Vec<WalletSummary>> {
        if names.len() > 10000 {
            return Err(Failure::Catalog);
        }
        for entry in &names {
            entry.validate()?;
        }
        names.sort_by(|a, b| a.name.cmp(&b.name));
        names.dedup_by(|a, b| a.name == b.name);
        Ok(names)
    }
    pub fn load(&self) -> Result<Vec<WalletSummary>> {
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(Failure::Catalog),
        };
        let mut bytes = Vec::new();
        file.take(1_048_577)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::Catalog)?;
        if bytes.len() > 1_048_576 {
            return Err(Failure::Catalog);
        }
        let entries = serde_json::from_slice(&bytes).map_err(|_| Failure::Catalog)?;
        Self::validate(entries)
    }

    fn lock(&self) -> Result<File> {
        let parent = self.path.parent().ok_or(Failure::Catalog)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|_| Failure::Catalog)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(parent.join("wallets.lock"))
            .map_err(|_| Failure::Catalog)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(Failure::Catalog);
        }
        Ok(lock)
    }
    fn write(&self, names: Vec<WalletSummary>) -> Result<Vec<WalletSummary>> {
        let names = Self::validate(names)?;
        let bytes = serde_json::to_vec(&names).map_err(|_| Failure::Catalog)?;
        if bytes.len() > 1_048_576 {
            return Err(Failure::Catalog);
        }
        self.write_bytes(&bytes)?;
        Ok(names)
    }
    fn write_bytes(&self, bytes: &[u8]) -> Result<()> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Failure::Catalog)?
            .as_nanos();
        let temp = self
            .path
            .with_file_name(format!(".wallets-{}-{nonce}.tmp", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .map_err(|_| Failure::Catalog)?;
        let result = (|| {
            file.write_all(bytes).map_err(|_| Failure::Catalog)?;
            file.sync_all().map_err(|_| Failure::Catalog)?;
            fs::rename(&temp, &self.path).map_err(|_| Failure::Catalog)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
    // Prefill the settings editor during an upgrade. This file never authorizes reuse.
    pub fn legacy_authentication_ttl(&self) -> Result<Option<u64>> {
        let path = self.path.with_file_name("settings.json");
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Failure::Settings),
        };
        if !file.metadata().map_err(|_| Failure::Settings)?.is_file() {
            return Err(Failure::Settings);
        }
        let mut bytes = Vec::new();
        file.take(65)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::Settings)?;
        if bytes.len() > 64 {
            return Err(Failure::Settings);
        }
        serde_json::from_slice::<u64>(&bytes)
            .map(Some)
            .map_err(|_| Failure::Settings)
    }
    pub fn upsert(&self, entry: WalletSummary) -> Result<()> {
        entry.validate()?;
        let _lock = self.lock()?;
        let mut names = self.load()?;
        names.retain(|old| old.name != entry.name);
        names.push(entry);
        self.write(names).map(|_| ())
    }
    pub fn remove(&self, name: &str) -> Result<()> {
        validate_name(name).map_err(|_| Failure::Catalog)?;
        let _lock = self.lock()?;
        let mut names = self.load()?;
        names.retain(|n| n.name != name);
        self.write(names).map(|_| ())
    }
    pub fn sync(
        &self,
        authenticated_entries: impl FnOnce() -> Result<Vec<WalletSummary>>,
    ) -> Result<Vec<WalletSummary>> {
        let revision = self.revision()?;
        let entries = authenticated_entries()?;
        let _lock = self.lock()?;
        if self.revision()? != revision {
            return Err(Failure::CatalogChanged);
        }
        self.write(entries)
    }

    fn revision(&self) -> Result<Option<CatalogRevision>> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Failure::Catalog),
        };
        Ok(Some(CatalogRevision {
            device: metadata.dev(),
            inode: metadata.ino(),
            changed_at: metadata.ctime(),
            changed_nanos: metadata.ctime_nsec(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    #[test]
    fn legacy_authentication_settings_prefill_and_reject_invalid_files() {
        let catalog = fixture();
        assert_eq!(catalog.legacy_authentication_ttl().unwrap(), None);
        let path = catalog.path.with_file_name("settings.json");
        fs::create_dir_all(catalog.path.parent().unwrap()).unwrap();
        for seconds in [0, 60, 300, u64::MAX] {
            fs::write(&path, seconds.to_string()).unwrap();
            assert_eq!(catalog.legacy_authentication_ttl().unwrap(), Some(seconds));
        }
        for value in [
            "-1".to_owned(),
            "1.5".to_owned(),
            "18446744073709551616".to_owned(),
            "null".to_owned(),
            "300".to_owned() + &" ".repeat(100),
        ] {
            fs::write(&path, value).unwrap();
            assert!(catalog.legacy_authentication_ttl().is_err());
        }
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("missing-target", &path).unwrap();
        assert!(catalog.legacy_authentication_ttl().is_err());
        fs::remove_dir_all(catalog.path.parent().unwrap()).unwrap();
    }
    #[test]
    fn sync_preserves_writes_made_during_authentication() {
        let catalog = fixture();
        let result = std::thread::scope(|scope| {
            catalog.sync(|| {
                let (send, receive) = std::sync::mpsc::channel();
                let catalog = &catalog;
                scope.spawn(move || {
                    let _ = send.send(catalog.upsert(WalletSummary::named("new-wallet".into())));
                });
                receive.recv_timeout(std::time::Duration::from_secs(2))
                    .expect("a catalog writer must finish while sync waits for authentication")?;
                Ok(vec![WalletSummary::named("stale-snapshot".into())])
            })
        });
        assert_eq!(result, Err(Failure::CatalogChanged));
        assert_eq!(names(catalog.load().unwrap()), ["new-wallet"]);
        fs::remove_dir_all(catalog.path.parent().unwrap()).unwrap();
    }
    fn names(entries: Vec<WalletSummary>) -> Vec<String> {
        entries.into_iter().map(|e| e.name).collect()
    }
    fn fixture() -> Catalog {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Catalog {
            path: std::env::temp_dir()
                .join(format!(
                    "wallet-catalog-test-{}-{nonce}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ))
                .join("wallets.json"),
        }
    }
    #[test]
    fn public_metadata_persistence_sync_and_failure_do_not_touch_keys() {
        let c = fixture();
        assert!(c.load().unwrap().is_empty());
        c.upsert(WalletSummary::named("second".into())).unwrap();
        c.upsert(WalletSummary::named("first".into())).unwrap();
        c.upsert(WalletSummary::named("first".into())).unwrap();
        assert_eq!(names(c.load().unwrap()), ["first", "second"]);
        assert_eq!(
            fs::read_to_string(&c.path).unwrap(),
            r#"[{"name":"first"},{"name":"second"}]"#
        );
        assert_eq!(
            fs::metadata(&c.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(c.sync(|| Err(Failure::Cancelled)), Err(Failure::Cancelled));
        assert_eq!(names(c.load().unwrap()), ["first", "second"]);
        c.remove("first").unwrap();
        assert_eq!(names(c.load().unwrap()), ["second"]);
        fs::write(&c.path, b"invalid metadata").unwrap();
        assert_eq!(c.load(), Err(Failure::Catalog));
        assert!(c.upsert(WalletSummary::named("third".into())).is_err());
        assert_eq!(
            names(
                c.sync(|| Ok(vec![WalletSummary::named("restored".into())]))
                    .unwrap()
            ),
            ["restored"]
        );
        assert!(c
            .upsert(WalletSummary::named("invalid name".into()))
            .is_err());
        assert!(c
            .sync(|| Ok(vec![WalletSummary::named("invalid name".into())]))
            .is_err());
        assert_eq!(names(c.load().unwrap()), ["restored"]);
        fs::remove_dir_all(c.path.parent().unwrap()).unwrap();
    }
    #[test]
    fn concurrent_updates_and_untrusted_files() {
        let c = fixture();
        std::thread::scope(|scope| {
            for i in 0..8 {
                let c = &c;
                scope.spawn(move || {
                    c.upsert(WalletSummary::named(format!("wallet-{i}")))
                        .unwrap()
                });
            }
        });
        assert_eq!(c.load().unwrap().len(), 8);
        fs::write(&c.path, br#"["../bad"]"#).unwrap();
        assert_eq!(c.load(), Err(Failure::Catalog));
        fs::write(&c.path, vec![b' '; 1_048_577]).unwrap();
        assert_eq!(c.load(), Err(Failure::Catalog));
        let link = c.path.with_file_name("link.json");
        std::os::unix::fs::symlink(&c.path, &link).unwrap();
        assert_eq!(Catalog { path: link }.load(), Err(Failure::Catalog));
        assert!(Catalog::for_group("../bad").is_err());
        fs::remove_dir_all(c.path.parent().unwrap()).unwrap();
    }
    #[test]
    fn public_addresses_persist_without_secret_fields() {
        let c = fixture();
        let wallet = crate::model::StoredWallet {
            version: 1,
            mnemonic: "test test test test test test test test test test test junk".into(),
            passphrase: String::new(),
        };
        let summary = WalletSummary::from_wallet("public-test", &wallet).unwrap();
        c.upsert(summary.clone()).unwrap();
        assert_eq!(c.load().unwrap(), [summary]);
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(&c.path).unwrap()).unwrap();
        let keys: Vec<_> = stored[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["ethereumAddress", "name", "solanaAddress"]);
        let mut malformed = c.load().unwrap().remove(0);
        malformed.solana_address = Some("not-a-solana-address".into());
        assert!(c.upsert(malformed).is_err());
        assert_eq!(c.load().unwrap().len(), 1);
        fs::remove_dir_all(c.path.parent().unwrap()).unwrap();
    }
}
