use std::time::SystemTime;

fn system_zone_offset_seconds() -> i32 {
    let output = std::process::Command::new("date")
        .arg("+%z")
        .output()
        .unwrap_or_else(|err| panic!("date +%z: {err}"));
    assert!(
        output.status.success(),
        "date +%z exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    assert!(
        text.len() == 5 && (text.starts_with('+') || text.starts_with('-')),
        "date +%z unexpected: {text:?}"
    );
    let sign = if text.starts_with('-') { -1 } else { 1 };
    let hours: i32 = text[1..3]
        .parse()
        .unwrap_or_else(|err| panic!("hours in {text:?}: {err}"));
    let minutes: i32 = text[3..5]
        .parse()
        .unwrap_or_else(|err| panic!("minutes in {text:?}: {err}"));
    sign * (hours * 3600 + minutes * 60)
}

#[test]
fn offset_matches_the_system_zone() {
    let expected = system_zone_offset_seconds();
    let got = navori_macos::local_utc_offset(SystemTime::now());
    assert_eq!(got, expected);
}
