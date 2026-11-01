//! Secrets in free text are replaced by `[REDACTED:<rule>]`; everything else is preserved.
//!
//! Fixture secrets below are fake but shaped like real ones. Provider-specific prefixes are
//! split with `concat!` so secret scanners (e.g. GitHub push protection) do not flag them.

use navori_redact::redact_text;

fn assert_redacted(input: &str, secret: &str, rule: &str) {
    let out = redact_text(input);
    assert!(
        !out.text.contains(secret),
        "secret leaked for rule {rule}: {}",
        out.text
    );
    assert!(
        out.text.contains(&format!("[REDACTED:{rule}]")),
        "expected marker for {rule}: {}",
        out.text
    );
    assert!(
        out.findings.iter().any(|f| f.rule == rule),
        "finding missing for {rule}"
    );
}

#[test]
fn plain_prose_is_left_untouched() {
    let input = "Refactor the token bucket in src/rate.rs; password reset flow ships Friday.";
    let out = redact_text(input);
    assert_eq!(out.text, input);
    assert!(out.findings.is_empty());
}

#[test]
fn github_classic_and_fine_grained_tokens_are_redacted() {
    let classic = "ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8";
    assert_redacted(
        &format!("export GH={classic} done"),
        classic,
        "github_token",
    );
    let fine =
        "github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRS";
    assert_redacted(&format!("token: {fine}"), fine, "github_token");
}

#[test]
fn anthropic_keys_are_redacted_as_anthropic_not_generic() {
    let key = concat!(
        "sk-",
        "ant-api03-",
        "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_abcdEFGH"
    );
    let out = redact_text(&format!("key {key}"));
    assert!(!out.text.contains(key));
    assert_eq!(out.findings.len(), 1);
    assert_eq!(out.findings[0].rule, "anthropic_key");
}

#[test]
fn openai_style_sk_keys_are_redacted() {
    let key = concat!("sk-", "proj-", "Ab12Cd34Ef56Gh78Ij90Kl12Mn34Op56Qr78");
    assert_redacted(&format!("OPENAI={key}"), key, "sk_api_key");
}

#[test]
fn aws_access_key_ids_are_redacted() {
    assert_redacted(
        "id AKIAIOSFODNN7EXAMPLE here",
        "AKIAIOSFODNN7EXAMPLE",
        "aws_access_key",
    );
}

#[test]
fn slack_stripe_google_and_xai_keys_are_redacted() {
    let slack = concat!("xox", "b-1234567890-", "abcdefghijkl");
    assert_redacted(slack, slack, "slack_token");
    let stripe = concat!("sk_", "live_", "4eC39HqLyjWDarjtT1zdp7dc");
    assert_redacted(stripe, stripe, "stripe_key");
    let google = "AIzaSyA1234567890abcdefghijklmnopqrstuv";
    assert_redacted(&format!("k={google}"), google, "google_api_key");
    let xai = "xai-AbCdEfGhIjKlMnOpQrStUvWxYz012345";
    assert_redacted(xai, xai, "xai_key");
}

#[test]
fn jwts_are_redacted() {
    let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
    assert_redacted(&format!("Authorization {jwt}"), jwt, "jwt");
}

#[test]
fn bearer_tokens_are_redacted() {
    let tok = "abcDEF123456ghiJKL789012mnoPQR";
    assert_redacted(&format!("Authorization: Bearer {tok}"), tok, "bearer_token");
}

#[test]
fn private_key_blocks_are_redacted_whole() {
    let block = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ\n-----END OPENSSH PRIVATE KEY-----";
    let out = redact_text(&format!("before\n{block}\nafter"));
    assert!(
        !out.text
            .contains("b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ")
    );
    assert!(out.text.starts_with("before\n"));
    assert!(out.text.ends_with("\nafter"));
    assert_eq!(out.findings[0].rule, "private_key");
}

#[test]
fn unterminated_private_key_block_is_redacted_to_the_end() {
    let out = redact_text("-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEAtruncated");
    assert!(!out.text.contains("MIIEowIBAAKCAQEA"));
}

#[test]
fn passwords_inside_urls_are_redacted_but_the_url_shape_stays() {
    let out = redact_text("DATABASE_URL_HOST postgres://navori:s3cr3tPass@localhost:5432/app");
    assert!(!out.text.contains("s3cr3tPass"));
    assert!(out.text.contains("postgres://navori:"));
    assert!(out.text.contains("@localhost:5432/app"));
}

#[test]
fn values_of_secret_looking_assignments_are_redacted() {
    let out = redact_text("DB_PASSWORD=hunter2hunter2\nAPI_KEY: \"zz9-plural-z-alpha\"\nPORT=5432");
    assert!(!out.text.contains("hunter2hunter2"));
    assert!(!out.text.contains("zz9-plural-z-alpha"));
    assert!(out.text.contains("DB_PASSWORD="));
    assert!(out.text.contains("PORT=5432"));
}

#[test]
fn several_secrets_in_one_text_are_all_redacted_with_original_offsets() {
    let a = "AKIAIOSFODNN7EXAMPLE";
    let b = "ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8";
    let input = format!("{a} and {b}");
    let out = redact_text(&input);
    assert_eq!(out.findings.len(), 2);
    assert_eq!(&input[out.findings[0].start..out.findings[0].end], a);
    assert_eq!(&input[out.findings[1].start..out.findings[1].end], b);
    assert_eq!(
        out.text,
        "[REDACTED:aws_access_key] and [REDACTED:github_token]"
    );
}
