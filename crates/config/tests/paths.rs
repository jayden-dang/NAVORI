use std::path::Path;

use navori_config::{Paths, Profile};

#[test]
fn profiles_parse_from_their_names() {
    assert_eq!(Profile::parse("release"), Some(Profile::Release));
    assert_eq!(Profile::parse("dev"), Some(Profile::Dev));
    assert_eq!(Profile::parse("prod"), None);
    assert_eq!(Profile::Dev.as_str(), "dev");
    assert_eq!(Profile::Release.as_str(), "release");
}

#[test]
fn release_profile_uses_standard_macos_locations() {
    let p = Paths::new(Profile::Release, Path::new("/Users/me"), 501);
    assert_eq!(
        p.data_dir,
        Path::new("/Users/me/Library/Application Support/Navori")
    );
    assert_eq!(p.logs_dir, Path::new("/Users/me/Library/Logs/Navori"));
    assert_eq!(
        p.config_file,
        Path::new("/Users/me/Library/Application Support/Navori/config.toml")
    );
    assert_eq!(
        p.db_file,
        Path::new("/Users/me/Library/Application Support/Navori/navori.db")
    );
    assert_eq!(
        p.backups_dir,
        Path::new("/Users/me/Library/Application Support/Navori/backups")
    );
    assert_eq!(
        p.socket,
        Path::new("/Users/me/Library/Application Support/Navori/run/navori.sock")
    );
}

#[test]
fn dev_profile_never_shares_files_with_release() {
    let home = Path::new("/Users/me");
    let rel = Paths::new(Profile::Release, home, 501);
    let dev = Paths::new(Profile::Dev, home, 501);
    assert_eq!(
        dev.data_dir,
        Path::new("/Users/me/Library/Application Support/Navori Dev")
    );
    assert_eq!(dev.logs_dir, Path::new("/Users/me/Library/Logs/Navori Dev"));
    assert_ne!(rel.socket, dev.socket);
    assert_ne!(rel.db_file, dev.db_file);
}

#[test]
fn socket_falls_back_to_a_short_tmp_path_when_home_is_too_deep() {
    // macOS limits Unix socket paths to 104 bytes (sun_path).
    let deep = format!("/Users/{}", "x".repeat(80));
    let p = Paths::new(Profile::Dev, Path::new(&deep), 501);
    assert!(p.socket.as_os_str().len() < 104);
    // A private per-user directory (made 0700 by the IPC server), never /tmp itself.
    assert_eq!(p.socket, Path::new("/tmp/navori-501/dev.sock"));
}
