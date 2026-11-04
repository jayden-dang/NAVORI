use anyhow::{Context, Result, bail};
use serde_json::Value;

/// What a perf run saw of Docker through `ports.list`, and whether a baseline may come from it.
#[derive(Debug, PartialEq, Eq)]
pub struct DockerReport {
    pub status: String,
    pub warning: Option<String>,
}

impl DockerReport {
    pub fn allows_record(&self) -> bool {
        self.warning.is_none()
    }
}

/// Reads the `docker` status out of a `ports.list` reply. A linked socket whose status is not
/// `read` means the run never read Docker.
pub fn report(linked: bool, ports_list: &str) -> Result<DockerReport> {
    let reply: Value = serde_json::from_str(ports_list).context("ports.list is not JSON")?;
    let status = match reply.get("docker") {
        Some(Value::String(tag)) => tag.clone(),
        Some(Value::Object(map)) if map.len() == 1 => {
            map.keys().next().cloned().unwrap_or_default()
        }
        _ => bail!("ports.list has no docker status"),
    };
    let warning = (linked && status != "read").then(|| {
        format!(
            "docker={status} although the Docker socket is linked: this run never read Docker, so it cannot be recorded as a baseline"
        )
    });
    Ok(DockerReport { status, warning })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_status_is_reported_and_allows_a_baseline() {
        let seen = report(true, r#"{"docker":"read","groups":[]}"#).unwrap();
        assert_eq!(seen.status, "read");
        assert!(seen.allows_record());
    }

    #[test]
    fn a_linked_socket_that_was_never_read_warns_and_refuses_a_baseline() {
        for (reply, status) in [
            (r#"{"docker":"not_seen"}"#, "not_seen"),
            (
                r#"{"docker":{"unreadable":"Docker did not answer in time."}}"#,
                "unreadable",
            ),
        ] {
            let seen = report(true, reply).unwrap();
            assert_eq!(seen.status, status);
            assert!(!seen.allows_record());
            let warning = seen.warning.unwrap();
            assert!(warning.contains(status), "{warning}");
        }
    }

    #[test]
    fn without_a_linked_socket_the_status_is_reported_and_never_a_warning() {
        let seen = report(false, r#"{"docker":"not_seen"}"#).unwrap();
        assert_eq!(seen.status, "not_seen");
        assert!(seen.allows_record());
    }

    #[test]
    fn a_reply_without_a_docker_status_is_an_error() {
        assert!(report(true, r#"{"groups":[]}"#).is_err());
        assert!(report(true, "not json").is_err());
    }
}
