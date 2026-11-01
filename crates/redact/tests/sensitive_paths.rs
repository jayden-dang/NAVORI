use std::path::Path;

use navori_redact::is_sensitive_path;

const HOME: &str = "/Users/me";

fn sensitive(p: &str) -> bool {
    is_sensitive_path(Path::new(p), Path::new(HOME))
}

#[test]
fn credential_locations_under_home_are_sensitive() {
    for p in [
        "/Users/me/.ssh/id_ed25519",
        "/Users/me/.ssh/config",
        "/Users/me/.gnupg/private-keys-v1.d/x.key",
        "/Users/me/.aws/credentials",
        "/Users/me/.config/gh/hosts.yml",
        "/Users/me/.netrc",
        "/Users/me/.docker/config.json",
        "/Users/me/.kube/config",
        "/Users/me/Library/Keychains/login.keychain-db",
        "/Users/me/.claude/.credentials.json",
        "/Users/me/.grok/auth.json",
        "/Users/me/.codex/auth.json",
    ] {
        assert!(sensitive(p), "{p} should be sensitive");
    }
}

#[test]
fn env_files_and_key_material_are_sensitive_anywhere() {
    for p in [
        "/work/app/.env",
        "/work/app/.env.local",
        "/work/app/.env.production",
        "/tmp/server.pem",
        "/tmp/tls.key",
        "/tmp/cert.p12",
        "/tmp/store.jks",
        "/backup/id_rsa",
        "/backup/id_ed25519.pub.bak",
    ] {
        assert!(sensitive(p), "{p} should be sensitive");
    }
}

#[test]
fn ordinary_project_files_and_env_templates_are_not_sensitive() {
    for p in [
        "/work/app/src/main.rs",
        "/work/app/.env.example",
        "/work/app/.env.sample",
        "/work/app/.env.template",
        "/work/app/keyboard.rs",
        "/Users/me/.config/navori/config.toml",
        "/Users/me/Documents/ssh-notes.md",
    ] {
        assert!(!sensitive(p), "{p} should not be sensitive");
    }
}

#[test]
fn relative_traversal_cannot_dodge_the_check() {
    assert!(sensitive("/Users/me/work/../.ssh/id_rsa"));
    assert!(sensitive("/Users/me/./.aws/credentials"));
}
