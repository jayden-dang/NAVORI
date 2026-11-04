const HELPER_TAIL: [&str; 3] = ["Contents", "Helpers", "navori"];

/// True when `command` is Navori's hook command for the given profile: the guarded form, whose
/// three program paths are one and the same, or the single-quoted form earlier versions wrote.
pub(crate) fn is_navori_command(command: &str, dev: bool) -> bool {
    let arguments = if dev { " --profile dev hook" } else { " hook" };
    guarded_program(command, arguments).is_some_and(|program| ends_in_helper(&program))
        || earlier_program(command, arguments).is_some_and(|program| ends_in_helper(&program))
}

fn earlier_program(command: &str, arguments: &str) -> Option<String> {
    let (program, rest) = unquote(command)?;
    (rest == arguments).then_some(program)
}

fn guarded_program(command: &str, arguments: &str) -> Option<String> {
    let rest = command.strip_prefix("[ -f ")?;
    let (tested, rest) = unquote(rest)?;
    let rest = rest.strip_prefix(" ] && [ -x ")?;
    let (executable, rest) = unquote(rest)?;
    let rest = rest.strip_prefix(" ] && exec ")?;
    let (program, rest) = unquote(rest)?;
    let tail = rest.strip_prefix(arguments)?;
    (tail == " || true" && tested == program && executable == program).then_some(program)
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
