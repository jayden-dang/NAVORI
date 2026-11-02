use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mark {
    StatusItemDrawn { ms: f64 },
    MenuOpened { ms: f64, first: bool, modules: u32 },
    SubmenuOpened { ms: f64 },
}

#[derive(Debug)]
pub struct LogStream {
    lines: Receiver<String>,
    child: Option<Child>,
}

pub fn parse_line(line: &str) -> Option<(u32, Mark)> {
    let value: Value = serde_json::from_str(line).ok()?;
    let pid = u32::try_from(value.get("processID")?.as_u64()?).ok()?;
    let message = value.get("eventMessage")?.as_str()?;
    Some((pid, parse_message(message)?))
}

fn parse_message(message: &str) -> Option<Mark> {
    let mut parts = message.split_whitespace();
    match parts.next()? {
        "statusItemDrawn" => {
            let ms = finite_ms(parts.next()?)?;
            if parts.next().is_some() {
                return None;
            }
            Some(Mark::StatusItemDrawn { ms })
        }
        "menuOpened" => {
            let ms = finite_ms(parts.next()?)?;
            let first = keyed::<bool>(parts.next()?, "first")?;
            let modules = keyed::<u32>(parts.next()?, "modules")?;
            if parts.next().is_some() {
                return None;
            }
            Some(Mark::MenuOpened { ms, first, modules })
        }
        "submenuOpened" => {
            let ms = finite_ms(parts.next()?)?;
            if parts.next().is_some() {
                return None;
            }
            Some(Mark::SubmenuOpened { ms })
        }
        _ => None,
    }
}

fn finite_ms(token: &str) -> Option<f64> {
    let ms = keyed::<f64>(token, "ms")?;
    ms.is_finite().then_some(ms)
}

fn keyed<T: FromStr>(token: &str, key: &str) -> Option<T> {
    token.strip_prefix(key)?.strip_prefix('=')?.parse().ok()
}

impl LogStream {
    pub fn from_reader(reader: impl BufRead + Send + 'static) -> Self {
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self { lines, child: None }
    }

    pub fn start() -> Result<Self> {
        let mut child = Command::new("/usr/bin/log")
            .args([
                "stream",
                "--predicate",
                r#"subsystem == "dev.navori" AND category == "perf""#,
                "--style",
                "ndjson",
                "--level",
                "default",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn /usr/bin/log stream")?;
        let stdout = child.stdout.take().context("log stream stdout")?;
        let mut stream = Self::from_reader(BufReader::new(stdout));
        stream.child = Some(child);
        // The first line is the banner (`Filtering the log data using …`), not an event.
        match stream.lines.recv_timeout(Duration::from_secs(10)) {
            Ok(_) => Ok(stream),
            Err(RecvTimeoutError::Timeout) => bail!("log stream produced no banner within 10s"),
            Err(RecvTimeoutError::Disconnected) => bail!("log stream ended before its banner"),
        }
    }

    pub fn wait_for(
        &mut self,
        pid: u32,
        want: fn(&Mark) -> bool,
        timeout: Duration,
    ) -> Result<Option<Mark>> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            match self.lines.recv_timeout(remaining) {
                Ok(line) => {
                    if let Some((seen, mark)) = parse_line(&line)
                        && seen == pid
                        && want(&mark)
                    {
                        return Ok(Some(mark));
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Ok(None),
                Err(RecvTimeoutError::Disconnected) => bail!("log stream ended"),
            }
        }
    }
}

impl Drop for LogStream {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Err(err) = child.kill() {
            let text = err.to_string();
            if !text.contains("No such process") && !text.contains("can't kill an exited process") {
                eprintln!("perf log stream kill failed for pid {}: {err}", child.id());
            }
        }
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if started.elapsed() >= Duration::from_secs(2) => {
                    eprintln!(
                        "perf log stream pid {} is still running after kill",
                        child.id()
                    );
                    return;
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(err) => {
                    eprintln!("perf log stream wait failed for pid {}: {err}", child.id());
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::time::Duration;

    use super::{LogStream, Mark, parse_line};

    #[test]
    fn parses_status_item_drawn() {
        let line = r#"{"timezoneName":"","messageType":"Default","eventType":"logEvent","source":null,"subsystem":"dev.navori","category":"perf","eventMessage":"statusItemDrawn ms=142.5","processID":74784}"#;
        assert_eq!(
            parse_line(line),
            Some((74784, Mark::StatusItemDrawn { ms: 142.5 }))
        );
    }

    #[test]
    fn parses_menu_opened() {
        let line = r#"{"timezoneName":"","messageType":"Default","eventType":"logEvent","source":null,"subsystem":"dev.navori","category":"perf","eventMessage":"menuOpened ms=31.2 first=true modules=1","processID":74784}"#;
        assert_eq!(
            parse_line(line),
            Some((
                74784,
                Mark::MenuOpened {
                    ms: 31.2,
                    first: true,
                    modules: 1,
                },
            ))
        );
    }

    #[test]
    fn ignores_banner_and_finished_lines() {
        let banner =
            r#"Filtering the log data using "subsystem == "dev.navori" AND category == "perf"""#;
        assert_eq!(parse_line(banner), None);
        assert_eq!(parse_line(r#"{"count":3,"finished":1}"#), None);
    }

    #[test]
    fn ignores_malformed_messages() {
        let lines = [
            "",
            "not json",
            "{",
            r#"{"eventMessage":"statusItemDrawn ms=1.0"}"#,
            r#"{"processID":9}"#,
            r#"{"processID":"9","eventMessage":"statusItemDrawn ms=1.0"}"#,
            r#"{"processID":9,"eventMessage":1}"#,
            r#"{"processID":4294967296,"eventMessage":"statusItemDrawn ms=1.0"}"#,
            r#"{"processID":9,"eventMessage":"statusItemDrawn"}"#,
            r#"{"processID":9,"eventMessage":"statusItemDrawn ms=fast"}"#,
            r#"{"processID":9,"eventMessage":"statusItemDrawn ms=nan"}"#,
            r#"{"processID":9,"eventMessage":"menuOpened ms=1.0 first=true"}"#,
            r#"{"processID":9,"eventMessage":"menuOpened ms=1.0 first=yes modules=1"}"#,
            r#"{"processID":9,"eventMessage":"menuOpened ms=1.0 first=true modules=1 extra=1"}"#,
            r#"{"processID":9,"eventMessage":"something else"}"#,
        ];
        for line in lines {
            assert_eq!(parse_line(line), None, "{line}");
        }
    }

    #[test]
    fn wait_for_skips_marks_of_other_pids() {
        let raw = "\
{\"eventMessage\":\"menuOpened ms=1.0 first=true modules=2\",\"processID\":7}\n\
{\"eventMessage\":\"statusItemDrawn ms=10.0\",\"processID\":9}\n\
{\"eventMessage\":\"menuOpened ms=31.2 first=true modules=1\",\"processID\":9}\n";
        let mut stream = LogStream::from_reader(Cursor::new(raw.to_owned().into_bytes()));
        let mark = stream
            .wait_for(
                9,
                |mark| matches!(mark, Mark::MenuOpened { first: true, .. }),
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(
            mark,
            Some(Mark::MenuOpened {
                ms: 31.2,
                first: true,
                modules: 1,
            })
        );
    }

    #[test]
    fn parses_a_later_menu_open() {
        let line = r#"{"eventMessage":"menuOpened ms=11.7 first=false modules=4","processID":9}"#;
        assert_eq!(
            parse_line(line),
            Some((
                9,
                Mark::MenuOpened {
                    ms: 11.7,
                    first: false,
                    modules: 4,
                },
            ))
        );
    }

    #[test]
    fn a_submenu_mark_parses_its_time() {
        let line = r#"{"eventMessage":"submenuOpened ms=7.3","processID":9}"#;
        assert_eq!(parse_line(line), Some((9, Mark::SubmenuOpened { ms: 7.3 })));
    }

    #[test]
    fn a_submenu_mark_with_an_extra_field_is_rejected() {
        let line = r#"{"processID":9,"eventMessage":"submenuOpened ms=1.0 extra=1"}"#;
        assert_eq!(parse_line(line), None);
    }

    #[test]
    fn a_closed_stream_is_not_a_timeout() {
        let mut stream = LogStream::from_reader(Cursor::new(Vec::new()));
        let err = stream
            .wait_for(
                9,
                |mark| matches!(mark, Mark::MenuOpened { .. }),
                Duration::from_millis(200),
            )
            .unwrap_err();
        assert!(format!("{err:#}").contains("log stream ended"), "{err:#}");
    }

    #[test]
    fn wait_for_times_out_while_the_stream_stays_open() {
        let (tx, lines) = std::sync::mpsc::channel();
        let mut stream = LogStream { lines, child: None };
        let started = std::time::Instant::now();
        let mark = stream
            .wait_for(
                9,
                |mark| matches!(mark, Mark::MenuOpened { .. }),
                Duration::from_millis(40),
            )
            .unwrap();
        assert_eq!(mark, None);
        assert!(started.elapsed() >= Duration::from_millis(30));
        drop(tx);
    }
}
