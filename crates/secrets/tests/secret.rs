use navori_secrets::{KeychainService, Secret};

#[test]
fn debug_output_of_a_secret_is_exactly_redacted_alone_and_nested() {
    #[derive(Debug)]
    struct Holder {
        token: Secret,
    }
    let value = concat!("sk-", "ant-FAKEFAKEFAKE-0123456789");
    assert_eq!(format!("{:?}", Secret::new(value)), "Secret(<redacted>)");
    assert_eq!(format!("{:#?}", Secret::new(value)), "Secret(<redacted>)");
    let holder = Holder {
        token: Secret::new(value),
    };
    assert_eq!(
        format!("{holder:?}"),
        "Holder { token: Secret(<redacted>) }"
    );
    assert_eq!(
        format!("{:?}", Some(Secret::new(value))),
        "Some(Secret(<redacted>))"
    );
}

#[test]
fn the_text_of_a_secret_comes_only_through_expose() {
    assert_eq!(Secret::new("abc").expose(), "abc");
}

#[test]
fn the_release_and_dev_services_have_their_documented_names_and_differ() {
    assert_eq!(KeychainService::RELEASE.as_str(), "dev.navori");
    assert_eq!(KeychainService::DEV.as_str(), "dev.navori.dev");
    assert_ne!(KeychainService::RELEASE, KeychainService::DEV);
}
