#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use std::collections::HashMap;
use std::sync::Arc;

use common::{
    FIXTURE, SECOND, call, contract_errors, contract_exists_delete, contract_set_get, key,
};
use keyring_core::Error;
use keyring_core::api::CredentialStoreApi;
use keyring_core::mock::{Cred, Store};
use navori_secrets::{Deleted, KeychainService, KeychainStore, Operation, Secret, SecretStore};

fn mock_store(service: KeychainService) -> (Arc<Store>, KeychainStore) {
    let mock = Store::new().expect("mock store");
    let store = KeychainStore::with_backend(service, mock.clone());
    (mock, store)
}

/// Make the next call on the credential for (service, account) fail with `err`.
fn inject(mock: &Arc<Store>, service: &str, account: &str, err: Error) {
    let entry = mock.build(service, account, None).expect("entry");
    entry
        .as_any()
        .downcast_ref::<Cred>()
        .expect("mock credential")
        .set_error(err);
}

#[test]
fn the_keychain_store_keeps_replaces_checks_and_removes_a_secret() {
    let (mock, store) = mock_store(KeychainService::RELEASE);
    contract_set_get(&store, &key("provider", "anthropic"));
    // SECRETS-2.2: after the replace exactly one credential matches the service and account,
    // and it holds the second value. This only shows we address the same credential on a
    // replace; the real Keychain updating the item in place stays a manual-run item
    // (design decision 9, SECRETS-7.3). The mock does not prove it.
    let found = mock
        .search(&HashMap::from([
            ("service", "dev.navori"),
            ("user", "provider/anthropic"),
        ]))
        .expect("search");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].get_password().expect("read back"), SECOND);
    contract_exists_delete(&store, &key("provider", "openai"));
}

#[test]
fn a_secret_is_stored_under_the_service_and_account_the_store_was_built_with() {
    for (service, name) in [
        (KeychainService::RELEASE, "dev.navori"),
        (KeychainService::DEV, "dev.navori.dev"),
    ] {
        let (mock, store) = mock_store(service);
        store
            .set(&key("provider", "anthropic"), &Secret::new(FIXTURE))
            .expect("set");
        let entry = mock.build(name, "provider/anthropic", None).expect("entry");
        assert_eq!(entry.get_password().expect("read back"), FIXTURE);
    }
}

#[test]
fn a_secret_set_for_one_service_is_absent_for_the_other_in_both_directions() {
    let mock = Store::new().expect("mock store");
    let release = KeychainStore::with_backend(KeychainService::RELEASE, mock.clone());
    let dev = KeychainStore::with_backend(KeychainService::DEV, mock.clone());
    let (to_dev, to_release) = (key("github", "token"), key("provider", "openai"));
    release
        .set(&to_dev, &Secret::new(FIXTURE))
        .expect("set release");
    assert!(dev.get(&to_dev).expect("dev get").is_none());
    assert!(!dev.exists(&to_dev).expect("dev exists"));
    dev.set(&to_release, &Secret::new(FIXTURE))
        .expect("set dev");
    assert!(release.get(&to_release).expect("release get").is_none());
    assert!(!release.exists(&to_release).expect("release exists"));
}

#[test]
fn a_backend_failure_of_each_call_is_an_error_naming_the_key_and_never_the_value() {
    let (mock, store) = mock_store(KeychainService::DEV);
    contract_errors(
        &store,
        &key("github", "token"),
        &["dev.navori.dev", "github", "token"],
        &|_op| {
            inject(
                &mock,
                "dev.navori.dev",
                "github/token",
                Error::PlatformFailure("locked".into()),
            )
        },
    );
}

#[test]
fn every_backend_error_except_no_entry_is_an_error_never_an_absent_or_false_outcome() {
    let (mock, store) = mock_store(KeychainService::RELEASE);
    let k = key("provider", "anthropic");
    store.set(&k, &Secret::new(FIXTURE)).expect("set");
    let kinds: [fn() -> Error; 10] = [
        || Error::PlatformFailure("boom".into()),
        || Error::NoStorageAccess("denied".into()),
        || Error::BadEncoding(vec![0xff]),
        || Error::BadDataFormat(vec![0xff], "odd".into()),
        || Error::BadStoreFormat("odd".into()),
        || Error::TooLong("service".into(), 8),
        || Error::Invalid("service".into(), "odd".into()),
        || Error::Ambiguous(Vec::new()),
        || Error::NoDefaultStore,
        || Error::NotSupportedByStore("odd".into()),
    ];
    for make in kinds {
        for op in [
            Operation::Set,
            Operation::Get,
            Operation::Exists,
            Operation::Delete,
        ] {
            inject(&mock, "dev.navori", "provider/anthropic", make());
            let err = call(&store, op, &k).expect_err("a backend error must stay an error");
            assert_eq!(err.operation(), op);
        }
    }
}

// SECRETS-2.5. Not covered: a failure of `build()` itself; the mock store cannot be made to
// fail it.
#[test]
fn a_backend_no_entry_is_absent_on_get_exists_and_delete_but_an_error_on_set() {
    let (mock, store) = mock_store(KeychainService::RELEASE);
    let k = key("provider", "anthropic");
    store.set(&k, &Secret::new(FIXTURE)).expect("set");
    let no_entry = || inject(&mock, "dev.navori", "provider/anthropic", Error::NoEntry);

    no_entry();
    let err = store
        .set(&k, &Secret::new(SECOND))
        .expect_err("NoEntry on set is a failure");
    assert_eq!(err.operation(), Operation::Set);
    // the failed set wrote nothing
    let read = store.get(&k).expect("get").expect("present");
    assert_eq!(read.expose(), FIXTURE);

    no_entry();
    assert!(store.get(&k).expect("get").is_none());
    no_entry();
    assert!(!store.exists(&k).expect("exists"));
    no_entry();
    assert_eq!(store.delete(&k).expect("delete"), Deleted::WasAbsent);
    // each injected error was consumed by its call and nothing was removed
    assert!(store.exists(&k).expect("exists after"));
}

#[test]
fn a_stored_value_carried_inside_a_backend_error_never_reaches_our_error() {
    let (mock, store) = mock_store(KeychainService::RELEASE);
    let k = key("provider", "anthropic");
    store.set(&k, &Secret::new(FIXTURE)).expect("set");
    inject(
        &mock,
        "dev.navori",
        "provider/anthropic",
        Error::BadEncoding(FIXTURE.as_bytes().to_vec()),
    );
    let err = store.get(&k).expect_err("bad encoding");
    assert_eq!(err.operation(), Operation::Get);
    for shown in [format!("{err}"), format!("{err:?}")] {
        assert!(
            !shown.contains(FIXTURE) && !shown.contains("FAKEFAKE"),
            "{shown}"
        );
    }
}

#[test]
fn an_item_that_exists_but_is_not_text_still_counts_as_set() {
    let (mock, store) = mock_store(KeychainService::RELEASE);
    let entry = mock
        .build("dev.navori", "provider/odd", None)
        .expect("entry");
    entry.set_secret(&[0xff, 0xfe, 0x00]).expect("raw bytes");
    assert!(store.exists(&key("provider", "odd")).expect("exists"));
}

#[test]
fn the_debug_output_of_the_store_names_the_service_and_no_value() {
    let (_mock, store) = mock_store(KeychainService::DEV);
    store
        .set(&key("github", "token"), &Secret::new(FIXTURE))
        .expect("set");
    let shown = format!("{store:?}");
    assert!(shown.contains("dev.navori.dev"));
    assert!(
        !shown.contains(FIXTURE) && !shown.contains("FAKEFAKE"),
        "{shown}"
    );
}
