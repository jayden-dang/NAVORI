//! The provider key calls of `NavoriCore`, driven with a fake Keychain.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use navori_config::Profile;
use navori_context::{SendError, Sender};
use navori_ffi::{AiDeps, CoreConfig, CoreError, CoreSnapshot, NavoriCore, SnapshotListener};
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use navori_secrets::{KeychainService, Operation, Secret, SecretKey, SecretStore};

struct Quiet;

impl SnapshotListener for Quiet {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

struct StubSender;

impl Sender for StubSender {
    fn send<'a>(
        &'a self,
        _: &'a Secret,
        _: &'a str,
        _: &'a str,
    ) -> BoxFuture<'a, Result<String, SendError>> {
        Box::pin(async { Ok("stub".to_owned()) })
    }
}

/// A short home path so the socket stays under the 104-byte `sun_path` limit.
fn home() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap()
}

fn config(home: &tempfile::TempDir, profile: &str) -> CoreConfig {
    CoreConfig {
        profile: profile.into(),
        home_dir: home.path().to_string_lossy().into_owned(),
        uid: std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(home.path()).unwrap()),
        app_version: "0.1.0-test".into(),
        log_to_file: false,
        helper_path: home
            .path()
            .join("Navori Dev.app/Contents/Helpers/navori")
            .to_string_lossy()
            .into_owned(),
    }
}

/// A fake Keychain whose factory counts the opens and remembers the service each one asked for.
struct Fixture {
    vault: FakeKeychain,
    opens: Arc<AtomicUsize>,
    services: Arc<Mutex<Vec<&'static str>>>,
}

impl Fixture {
    fn new() -> Self {
        Self {
            vault: FakeKeychain::new(),
            opens: Arc::new(AtomicUsize::new(0)),
            services: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn deps(&self) -> AiDeps {
        let vault = self.vault.clone();
        let opens = self.opens.clone();
        let services = self.services.clone();
        AiDeps {
            open_secrets: Arc::new(move |service| {
                opens.fetch_add(1, Ordering::SeqCst);
                services.lock().unwrap().push(service.as_str());
                let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(service, &vault));
                Ok(store)
            }),
            make_sender: Arc::new(|| Arc::new(StubSender)),
        }
    }

    fn core(&self, home: &tempfile::TempDir, profile: &str) -> Arc<NavoriCore> {
        NavoriCore::start_with(config(home, profile), Arc::new(Quiet), self.deps()).unwrap()
    }

    fn opened(&self) -> usize {
        self.opens.load(Ordering::SeqCst)
    }

    fn stored_in(&self, service: KeychainService) -> Option<String> {
        MemoryStore::new(service, &self.vault)
            .get(&slot())
            .unwrap()
            .map(|secret| secret.expose().to_owned())
    }
}

fn slot() -> SecretKey {
    SecretKey::new("llm-provider", "deepseek").unwrap()
}

#[test]
fn the_profile_maps_to_its_keychain_service() {
    assert_eq!(
        navori_ffi::service_for(Profile::Release).as_str(),
        "dev.navori"
    );
    assert_eq!(
        navori_ffi::service_for(Profile::Dev).as_str(),
        "dev.navori.dev"
    );
}

#[test]
fn a_saved_key_is_stored_trimmed_under_the_profile_service() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    futures::executor::block_on(core.save_provider_key("  k1\n".into())).unwrap();

    assert_eq!(
        fixture.stored_in(KeychainService::DEV).as_deref(),
        Some("k1")
    );
    assert_eq!(fixture.stored_in(KeychainService::RELEASE), None);
    assert_eq!(fixture.vault.item_count(), 1);
}

#[test]
fn each_profile_opens_its_own_service() {
    let dev_fixture = Fixture::new();
    let release_fixture = Fixture::new();
    let (dev_home, release_home) = (home(), home());
    let dev = dev_fixture.core(&dev_home, "dev");
    let release = release_fixture.core(&release_home, "release");

    futures::executor::block_on(dev.save_provider_key("k1".into())).unwrap();
    futures::executor::block_on(release.save_provider_key("k1".into())).unwrap();

    assert_eq!(*dev_fixture.services.lock().unwrap(), ["dev.navori.dev"]);
    assert_eq!(*release_fixture.services.lock().unwrap(), ["dev.navori"]);
}

#[test]
fn saving_replaces_the_earlier_key() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();
    futures::executor::block_on(core.save_provider_key("k2".into())).unwrap();

    assert_eq!(
        fixture.stored_in(KeychainService::DEV).as_deref(),
        Some("k2")
    );
    assert_eq!(fixture.vault.item_count(), 1);
    assert_eq!(fixture.opened(), 1);
}

#[test]
fn an_empty_key_makes_no_keychain_call() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    let err = futures::executor::block_on(core.save_provider_key("   ".into())).unwrap_err();

    let CoreError::Secrets { message } = err else {
        panic!("expected a Keychain error, got {err:?}");
    };
    assert!(message.contains("Nothing was entered"), "{message}");
    assert_eq!(fixture.opened(), 0);
    assert_eq!(fixture.vault.item_count(), 0);
}

#[test]
fn a_keychain_failure_reaches_the_message() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");
    fixture.vault.fail_next(Operation::Set);

    let err = futures::executor::block_on(core.save_provider_key("k1".into())).unwrap_err();

    let CoreError::Secrets { message } = err else {
        panic!("expected a Keychain error, got {err:?}");
    };
    assert_eq!(
        message,
        "set failed for llm-provider/deepseek in service dev.navori.dev; the Keychain could not complete the call (it may be locked, access may be denied, or the item may be unreadable), so check Keychain Access and retry"
    );
    assert!(!message.contains("no key"));
}

#[test]
fn a_failed_remove_reaches_the_message() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");
    fixture.vault.fail_next(Operation::Delete);

    let err = futures::executor::block_on(core.remove_provider_key()).unwrap_err();

    let CoreError::Secrets { message } = err else {
        panic!("expected a Keychain error, got {err:?}");
    };
    assert!(
        message.starts_with("delete failed for llm-provider/deepseek"),
        "{message}"
    );
}

#[test]
fn remove_reports_whether_a_key_was_stored() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();

    assert_eq!(
        futures::executor::block_on(core.remove_provider_key()),
        Ok(true)
    );
    assert_eq!(
        futures::executor::block_on(core.remove_provider_key()),
        Ok(false)
    );
    assert_eq!(fixture.vault.item_count(), 0);
}

#[test]
fn remove_leaves_the_other_profiles_key() {
    let fixture = Fixture::new();
    let (dev_home, release_home) = (home(), home());
    let dev = fixture.core(&dev_home, "dev");
    let release = fixture.core(&release_home, "release");
    futures::executor::block_on(release.save_provider_key("k1".into())).unwrap();

    assert_eq!(
        futures::executor::block_on(dev.remove_provider_key()),
        Ok(false)
    );

    assert_eq!(
        fixture.stored_in(KeychainService::RELEASE).as_deref(),
        Some("k1")
    );
}

#[test]
fn starting_the_core_opens_no_keychain() {
    let fixture = Fixture::new();
    let home = home();
    let _core = fixture.core(&home, "dev");

    assert_eq!(fixture.opened(), 0);
}

#[test]
fn a_failed_open_is_retried_by_the_next_call() {
    let opens = Arc::new(AtomicUsize::new(0));
    let vault = FakeKeychain::new();
    let deps = AiDeps {
        open_secrets: {
            let (opens, vault) = (opens.clone(), vault.clone());
            Arc::new(move |service| {
                if opens.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(navori_secrets::OpenError);
                }
                let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(service, &vault));
                Ok(store)
            })
        },
        make_sender: Arc::new(|| Arc::new(StubSender)),
    };
    let home = home();
    let core = NavoriCore::start_with(config(&home, "dev"), Arc::new(Quiet), deps).unwrap();

    assert!(futures::executor::block_on(core.save_provider_key("k1".into())).is_err());
    futures::executor::block_on(core.save_provider_key("k1".into())).unwrap();

    assert_eq!(vault.item_count(), 1);
    assert_eq!(opens.load(Ordering::SeqCst), 2);
}

#[test]
fn the_key_calls_work_from_a_non_tokio_executor() {
    let fixture = Fixture::new();
    let home = home();
    let core = fixture.core(&home, "dev");

    futures::executor::block_on(async {
        core.save_provider_key("k1".into()).await.unwrap();
        assert_eq!(core.remove_provider_key().await, Ok(true));
    });
}

#[test]
fn the_key_text_reaches_no_log_config_or_database() {
    let planted = concat!("ghp_", "abcdefghijklmnopqrstuvwxyz0123456789");
    let fixture = Fixture::new();
    let home = home();
    let data = home.path().join("Library/Application Support/Navori Dev");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("config.toml"), "# nothing configured\n").unwrap();
    let core = fixture.core(&home, "dev");

    futures::executor::block_on(core.save_provider_key(planted.into())).unwrap();
    core.shutdown();

    let mut searched = 0;
    for entry in std::fs::read_dir(&data).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(planted.len())
                    .any(|w| w == planted.as_bytes()),
                "{} holds the key",
                path.display()
            );
            searched += 1;
        }
    }
    assert!(searched >= 2, "searched {searched} files");
}
