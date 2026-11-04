#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use navori_secrets::{Deleted, Operation, Secret, SecretError, SecretKey, SecretStore};

pub const FIXTURE: &str = concat!("ghp_", "FAKEFAKEFAKEFAKE0123456789");
pub const SECOND: &str = concat!("ghp_", "SECONDSECOND9876543210");
/// A value that only a (wrongly) written failing `set` could leave behind.
pub const NEVER_STORED: &str = concat!("ghp_", "FAKENEVERSTORED5555");

pub fn key(kind: &str, name: &str) -> SecretKey {
    SecretKey::new(kind, name).expect("valid key")
}

/// Run one store operation and keep only its error.
pub fn call(store: &dyn SecretStore, op: Operation, key: &SecretKey) -> Result<(), SecretError> {
    call_with(store, op, key, FIXTURE)
}

/// Like [`call`], but a `set` carries `value`.
pub fn call_with(
    store: &dyn SecretStore,
    op: Operation,
    key: &SecretKey,
    value: &str,
) -> Result<(), SecretError> {
    match op {
        Operation::Set => store.set(key, &Secret::new(value)),
        Operation::Get => store.get(key).map(|_| ()),
        Operation::Exists => store.exists(key).map(|_| ()),
        Operation::Delete => store.delete(key).map(|_| ()),
    }
}

/// SECRETS-2.2 to 2.4. "Leaving one item" cannot be seen through `SecretStore`; each caller
/// counts items on its own backend after this returns.
pub fn contract_set_get(store: &dyn SecretStore, key: &SecretKey) {
    assert!(store.get(key).expect("get never set").is_none());
    store.set(key, &Secret::new(FIXTURE)).expect("set");
    assert_eq!(
        store.get(key).expect("get").expect("present").expose(),
        FIXTURE
    );
    store.set(key, &Secret::new(SECOND)).expect("replace");
    assert_eq!(
        store.get(key).expect("get").expect("present").expose(),
        SECOND
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
        let err = call_with(store, op, key, NEVER_STORED)
            .expect_err("an injected failure must be an error");
        assert_eq!(err.operation(), op);
        for shown in [format!("{err}"), format!("{err:?}")] {
            assert!(
                !shown.contains(FIXTURE)
                    && !shown.contains(NEVER_STORED)
                    && !shown.contains("FAKEFAKE"),
                "{shown}"
            );
            for part in shown_parts {
                assert!(shown.contains(part), "{part} missing from {shown}");
            }
        }
    }
    // Nothing a failed call did is visible: the old value is still stored (so the failed set
    // wrote nothing and the failed delete removed nothing), and each of the four calls now
    // succeeds, so no injected failure is left pending.
    assert_eq!(
        store.get(key).expect("recovers").expect("present").expose(),
        FIXTURE
    );
    assert!(store.exists(key).expect("exists recovers"));
    store.set(key, &Secret::new(SECOND)).expect("set recovers");
    assert_eq!(
        store.delete(key).expect("delete recovers"),
        Deleted::Removed
    );
}
