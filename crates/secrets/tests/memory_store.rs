mod common;
use common::{FIXTURE, contract_errors, contract_exists_delete, contract_set_get, key};
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use navori_secrets::{KeychainService, Secret, SecretStore};

#[test]
fn a_value_set_through_the_fake_reads_back_and_a_second_set_replaces_it() {
    let vault = FakeKeychain::new();
    let store = MemoryStore::new(KeychainService::RELEASE, &vault);
    contract_set_get(&store, &key("provider", "anthropic"));
}

#[test]
fn asking_whether_a_key_is_set_and_deleting_it_twice_behave_as_documented() {
    let vault = FakeKeychain::new();
    let store = MemoryStore::new(KeychainService::RELEASE, &vault);
    contract_exists_delete(&store, &key("provider", "anthropic"));
}

#[test]
fn an_injected_failure_of_each_call_is_an_error_naming_the_key_and_never_the_value() {
    let vault = FakeKeychain::new();
    let store = MemoryStore::new(KeychainService::DEV, &vault);
    contract_errors(
        &store,
        &key("github", "token"),
        &["dev.navori.dev", "github", "token"],
        &|op| vault.fail_next(op),
    );
}

#[test]
fn a_value_set_through_one_service_is_absent_through_the_other_in_both_directions() {
    let vault = FakeKeychain::new();
    let release = MemoryStore::new(KeychainService::RELEASE, &vault);
    let dev = MemoryStore::new(KeychainService::DEV, &vault);
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
fn the_debug_output_of_the_fake_and_its_store_never_contains_a_value() {
    let vault = FakeKeychain::new();
    let store = MemoryStore::new(KeychainService::RELEASE, &vault);
    store
        .set(&key("github", "token"), &Secret::new(FIXTURE))
        .expect("set");
    for shown in [format!("{vault:?}"), format!("{store:?}")] {
        assert!(
            !shown.contains(FIXTURE) && !shown.contains("FAKEFAKE"),
            "{shown}"
        );
    }
}
