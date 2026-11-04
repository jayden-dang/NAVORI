use anyhow::Result;
use serde_json::Value;

const NOT_LISTENING: &str = "Docker backend not listening; baseline excludes the Docker read";

/// What a perf run saw of Docker through `ports.list`, and whether a baseline may come from it.
#[derive(Debug, PartialEq, Eq)]
pub struct DockerReport {
    pub status: String,
    pub note: Option<String>,
    pub warning: Option<String>,
}

impl DockerReport {
    pub fn records(&self, record: bool, passed: bool) -> bool {
        record && passed && self.warning.is_none()
    }
}

/// Reads the `docker` status out of a `ports.list` reply, or `unavailable` when there is none.
/// With the socket linked, a baseline needs the status `read` or `not_seen`.
pub fn report(linked: bool, ports_list: Result<String>) -> DockerReport {
    let status = ports_list
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|reply| match reply.get("docker") {
            Some(Value::String(tag)) => Some(tag.clone()),
            Some(Value::Object(map)) if map.len() == 1 => map.keys().next().cloned(),
            _ => None,
        })
        .unwrap_or_else(|| "unavailable".to_owned());
    let refused = linked && matches!(status.as_str(), "unreadable" | "unavailable");
    let warning = refused.then(|| {
        format!("docker={status} although the Docker socket is linked: this run did not read Docker, so it cannot be recorded as a baseline")
    });
    let note = (linked && status == "not_seen").then(|| NOT_LISTENING.to_owned());
    DockerReport {
        status,
        note,
        warning,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(status: &str) -> Result<String> {
        match status {
            "unreadable" => {
                Ok(r#"{"docker":{"unreadable":"Docker did not answer in time."}}"#.to_owned())
            }
            "unavailable" => Err(anyhow::anyhow!("no ports module")),
            tag => Ok(format!(r#"{{"docker":"{tag}","groups":[]}}"#)),
        }
    }

    #[test]
    fn each_link_and_status_decides_the_note_the_warning_and_the_baseline() {
        // (linked, status, note, refuses)
        let table = [
            (true, "read", false, false),
            (true, "not_seen", true, false),
            (true, "unreadable", false, true),
            (true, "unavailable", false, true),
            (false, "read", false, false),
            (false, "not_seen", false, false),
            (false, "unreadable", false, false),
            (false, "unavailable", false, false),
        ];
        for (linked, status, note, refuses) in table {
            let seen = report(linked, reply(status));
            let label = format!("linked={linked} status={status}");
            assert_eq!(seen.status, status, "{label}");
            assert_eq!(seen.note.is_some(), note, "{label}");
            assert_eq!(seen.warning.is_some(), refuses, "{label}");
            assert_eq!(seen.records(true, true), !refuses, "{label}");
            assert!(!seen.records(false, true), "{label}");
            assert!(!seen.records(true, false), "{label}");
        }
    }

    #[test]
    fn a_reply_that_is_not_a_docker_status_is_unavailable() {
        for text in [r#"{"groups":[]}"#, "not json"] {
            assert_eq!(report(true, Ok(text.to_owned())).status, "unavailable");
        }
    }
}
