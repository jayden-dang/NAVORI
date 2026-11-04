#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use std::time::{SystemTime, UNIX_EPOCH};

use common::{contract_exists_delete, contract_set_get};
use navori_secrets::{KeychainService, KeychainStore, SecretKey, SecretStore};

/// Manual only. Writes to the login Keychain under service `dev.navori.dev`, kind `contract-test`,
/// and deletes what it wrote. A run that panics midway can leave an item named `run-<pid>-<nanos>`
/// there; remove it with Keychain Access.
/// Run: NAVORI_KEYCHAIN_CONTRACT=1 cargo test -p navori-secrets --test keychain_real -- --ignored
#[test]
#[ignore = "writes to the login Keychain; run by hand with NAVORI_KEYCHAIN_CONTRACT=1 and --ignored"]
fn the_real_keychain_honours_the_store_contract() {
    assert_eq!(
        std::env::var("NAVORI_KEYCHAIN_CONTRACT").as_deref(),
        Ok("1"),
        "set NAVORI_KEYCHAIN_CONTRACT=1 to run this test against the login Keychain"
    );
    let store = KeychainStore::open(KeychainService::DEV).expect("open the login keychain store");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let name = format!("run-{}-{stamp}", std::process::id());
    // get of a never-set key, replace in place, then exists, delete and a second delete
    let first = SecretKey::new("contract-test", &name).expect("key");
    contract_set_get(&store, &first);
    let second = SecretKey::new("contract-test", &format!("{name}-b")).expect("key");
    contract_exists_delete(&store, &second);
    store.delete(&first).expect("clean up the first key");
}
