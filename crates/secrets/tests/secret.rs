use navori_secrets::{KeychainService, Secret};

#[test]
fn debug_output_of_a_secret_never_contains_the_value() {
    let value = concat!("sk-", "ant-FAKEFAKEFAKE-0123456789");
    let shown = format!("{:?}", Secret::new(value));
    assert!(!shown.contains(value));
    assert!(!shown.contains("FAKEFAKE"));
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
