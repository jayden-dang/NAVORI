#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use navori_secrets::{Deleted, Operation, Secret, SecretError, SecretKey, SecretStore};

pub const FIXTURE: &str = concat!("ghp_", "FAKEFAKEFAKEFAKE0123456789");

pub fn key(kind: &str, name: &str) -> SecretKey {
    SecretKey::new(kind, name).expect("valid key")
}

/// Run one store operation and keep only its error.
pub fn call(store: &dyn SecretStore, op: Operation, key: &SecretKey) -> Result<(), SecretError> {
    match op {
        Operation::Set => store.set(key, &Secret::new(FIXTURE)),
        Operation::Get => store.get(key).map(|_| ()),
        Operation::Exists => store.exists(key).map(|_| ()),
        Operation::Delete => store.delete(key).map(|_| ()),
    }
}

/// SECRETS-2.2 to 2.4.
pub fn contract_set_get(store: &dyn SecretStore, key: &SecretKey) {
    assert!(store.get(key).expect("get never set").is_none());
    store.set(key, &Secret::new(FIXTURE)).expect("set");
    assert_eq!(
        store.get(key).expect("get").expect("present").expose(),
        FIXTURE
    );
    let second = concat!("ghp_", "SECONDSECOND9876543210");
    store.set(key, &Secret::new(second)).expect("replace");
    assert_eq!(
        store.get(key).expect("get").expect("present").expose(),
        second
    );
}

/// SECRETS-3.1, 4.1, 4.2.
pub fn contract_exists_delete(store: &dyn SecretStore, key: &SecretKey) {
    assert!(!store.exists(key).expect("exists before"));
    assert_eq!(
        store.delete(key).expect("delete nothing"),
        Deleted::WasAbsent
    );
    store.set(key, &Secret::new(FIXTURE)).expect("set");
    assert!(store.exists(key).expect("exists after"));
    assert_eq!(store.delete(key).expect("delete"), Deleted::Removed);
    assert!(!store.exists(key).expect("exists gone"));
    assert!(store.get(key).expect("get gone").is_none());
    assert_eq!(store.delete(key).expect("delete twice"), Deleted::WasAbsent);
}

/// SECRETS-2.5 and 6.3: a failure of any of the four calls is an error naming the call and
/// every string in `shown_parts`, and never the value. `inject(op)` makes the next `op` fail.
pub fn contract_errors(
    store: &dyn SecretStore,
    key: &SecretKey,
    shown_parts: &[&str],
    inject: &dyn Fn(Operation),
) {
    store.set(key, &Secret::new(FIXTURE)).expect("set");
    for op in [
        Operation::Set,
        Operation::Get,
        Operation::Exists,
        Operation::Delete,
    ] {
        inject(op);
        let err = call(store, op, key).expect_err("an injected failure must be an error");
        assert_eq!(err.operation(), op);
        for shown in [format!("{err}"), format!("{err:?}")] {
            assert!(
                !shown.contains(FIXTURE) && !shown.contains("FAKEFAKE"),
                "{shown}"
            );
            for part in shown_parts {
                assert!(shown.contains(part), "{part} missing from {shown}");
            }
        }
    }
    // a failed delete removed nothing
    assert_eq!(
        store.get(key).expect("recovers").expect("present").expose(),
        FIXTURE
    );
}
