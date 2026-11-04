use navori_secrets::{KeyPart, KeyProblem, SecretKey};

#[test]
fn a_provider_key_is_accepted() {
    assert!(SecretKey::new("provider", "anthropic").is_ok());
}

#[test]
fn a_name_with_a_slash_is_rejected_and_the_error_names_the_name() {
    let err = SecretKey::new("github", "a/b").expect_err("slash");
    assert_eq!((err.part, err.problem), (KeyPart::Name, KeyProblem::Slash));
    assert!(err.to_string().contains("name"));
}

#[test]
fn an_empty_part_a_slash_and_a_control_character_are_each_rejected() {
    let cases = [
        ("", "x", KeyPart::Kind, KeyProblem::Empty),
        ("a/b", "x", KeyPart::Kind, KeyProblem::Slash),
        ("kind", "", KeyPart::Name, KeyProblem::Empty),
        ("kind", "tab\there", KeyPart::Name, KeyProblem::Control),
        ("new\nline", "x", KeyPart::Kind, KeyProblem::Control),
        ("k", "del\u{7f}x", KeyPart::Name, KeyProblem::Control),
        ("k", "c1\u{85}x", KeyPart::Name, KeyProblem::Control),
        ("", "", KeyPart::Kind, KeyProblem::Empty),
        ("a/b", "t\tb", KeyPart::Kind, KeyProblem::Slash),
    ];
    for (kind, name, part, problem) in cases {
        let err = SecretKey::new(kind, name).expect_err("must be rejected");
        assert_eq!(
            (err.part, err.problem),
            (part, problem),
            "{kind:?}/{name:?}"
        );
    }
}

#[test]
fn each_rejection_says_which_part_and_what_is_wrong() {
    let cases = [
        ("", "x", "kind is empty"),
        ("a/b", "x", "kind contains `/`"),
        ("k", "", "name is empty"),
        ("k", "t\tb", "name contains a control character"),
    ];
    for (kind, name, expected) in cases {
        let shown = SecretKey::new(kind, name)
            .expect_err("rejected")
            .to_string();
        assert!(shown.contains(expected), "{shown}");
    }
}
