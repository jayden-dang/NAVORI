const HELPER_TAIL: [&str; 3] = ["Contents", "Helpers", "navori"];

/// True when `command` is one single-quoted program path ending `Contents/Helpers/navori`
/// followed by exactly the arguments of the given profile.
pub(crate) fn is_navori_command(command: &str, dev: bool) -> bool {
    let Some((program, rest)) = unquote(command) else {
        return false;
    };
    let expected = if dev { " --profile dev hook" } else { " hook" };
    rest == expected && ends_in_helper(&program)
}

fn ends_in_helper(program: &str) -> bool {
    let parts: Vec<&str> = program.split('/').collect();
    parts.len() >= HELPER_TAIL.len() && parts[parts.len() - HELPER_TAIL.len()..] == HELPER_TAIL
}

/// Reads one POSIX single-quoted word, where `'` inside is written `'\''`.
fn unquote(command: &str) -> Option<(String, &str)> {
    let mut word = String::new();
    let mut rest = command;
    loop {
        rest = rest.strip_prefix('\'')?;
        let end = rest.find('\'')?;
        word.push_str(&rest[..end]);
        rest = &rest[end + 1..];
        match rest.strip_prefix("\\'") {
            Some(after) => {
                word.push('\'');
                rest = after;
                if !rest.starts_with('\'') {
                    return None;
                }
            }
            None => return Some((word, rest)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_navori_command;

    #[test]
    fn release_and_dev_shapes_are_told_apart() {
        let release = "'/A/Navori.app/Contents/Helpers/navori' hook";
        let dev = "'/A/Navori Dev.app/Contents/Helpers/navori' --profile dev hook";
        assert!(is_navori_command(release, false));
        assert!(!is_navori_command(release, true));
        assert!(is_navori_command(dev, true));
        assert!(!is_navori_command(dev, false));
    }

    #[test]
    fn a_quote_inside_the_path_is_read_back() {
        let command = "'/A/It'\\''s/Navori.app/Contents/Helpers/navori' hook";
        assert!(is_navori_command(command, false));
    }

    #[test]
    fn anything_else_is_foreign() {
        for command in [
            "\"/A/Navori.app/Contents/Helpers/navori\" hook",
            "/A/Navori.app/Contents/Helpers/navori hook",
            "'/A/Navori.app/Contents/Helpers/navori' hook extra",
            "'/A/Navori.app/Contents/Helpers/navori' hook; rm -rf x",
            "'/A/Navori.app/Contents/Helpers/other' hook",
            "'/A/Navori.app/Contents/Helpers/navori'",
            "'/A/Navori.app/Contents/Helpers/navori' --profile dev hook ",
            "'/A/Navori.app/Contents/Helpers/navori'\\",
            "''",
            "",
        ] {
            assert!(!is_navori_command(command, false), "{command}");
            assert!(!is_navori_command(command, true), "{command}");
        }
    }
}
