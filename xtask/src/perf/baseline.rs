use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};

use super::judge::{Kind, METRICS, Measurements, Verdict};

#[derive(Debug)]
pub struct Machine {
    pub model: String,
    pub macos: String,
}

impl Machine {
    pub fn current() -> Result<Machine> {
        Ok(Machine {
            model: command_line("/usr/sbin/sysctl", &["-n", "hw.model"])?,
            macos: command_line("/usr/bin/sw_vers", &["-productVersion"])?,
        })
    }
}

#[derive(Debug)]
pub struct Baseline {
    pub machine: Machine,
    pub revision: String,
    pub recorded: String,
    pub metrics: BTreeMap<String, (f64, f64)>,
}

impl Baseline {
    pub fn load(path: &Path) -> Result<Option<Baseline>> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()));
            }
        };
        let value: Value = serde_json::from_str(&text)
            .with_context(|| format!("invalid perf baseline {}", path.display()))?;
        parse_baseline(&value)
            .with_context(|| format!("invalid perf baseline {}", path.display()))
            .map(Some)
    }

    pub fn from_run(
        measurements: &Measurements,
        machine: Machine,
        revision: String,
        recorded: String,
    ) -> Baseline {
        let mut metrics = BTreeMap::new();
        for spec in METRICS {
            if matches!(spec.kind, Kind::Skipped { .. }) {
                continue;
            }
            if let Some(&value) = measurements.get(spec.name) {
                metrics.insert(spec.name.to_owned(), (value, spec.floor));
            }
        }
        Baseline {
            machine,
            revision,
            recorded,
            metrics,
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut bytes =
            serde_json::to_vec_pretty(&to_json(self)).context("cannot encode perf baseline")?;
        bytes.push(b'\n');
        fs::write(path, bytes).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(())
    }
}

pub fn finish(path: &Path, verdict: &Verdict, record: bool, baseline: &Baseline) -> Result<bool> {
    if !(record && verdict.passed) {
        return Ok(false);
    }
    baseline.save(path)?;
    Ok(true)
}

fn to_json(baseline: &Baseline) -> Value {
    let mut metrics = Map::new();
    for (name, (median, floor)) in &baseline.metrics {
        metrics.insert(name.clone(), json!({ "median": median, "floor": floor }));
    }
    json!({
        "machine": baseline.machine.model,
        "macos": baseline.machine.macos,
        "revision": baseline.revision,
        "recorded": baseline.recorded,
        "metrics": metrics,
    })
}

fn parse_baseline(value: &Value) -> Result<Baseline> {
    let metrics_value = value
        .get("metrics")
        .context("perf baseline missing `metrics`")?;
    let metrics_object = metrics_value
        .as_object()
        .context("perf baseline `metrics` must be an object")?;
    let mut metrics = BTreeMap::new();
    for (name, entry) in metrics_object {
        let median = number_field(entry, name, "median")?;
        let floor = number_field(entry, name, "floor")?;
        metrics.insert(name.clone(), (median, floor));
    }
    Ok(Baseline {
        machine: Machine {
            model: string_field(value, "machine")?,
            macos: string_field(value, "macos")?,
        },
        revision: string_field(value, "revision")?,
        recorded: string_field(value, "recorded")?,
        metrics,
    })
}

fn string_field(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("perf baseline missing string `{key}`"))
}

fn number_field(entry: &Value, metric: &str, key: &str) -> Result<f64> {
    entry
        .get(key)
        .and_then(Value::as_f64)
        .with_context(|| format!("perf baseline metric `{metric}` missing {key}"))
}

fn command_line(program: &str, args: &[&str]) -> Result<String> {
    let rendered = format!("{program} {}", args.join(" "));
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("cannot run {rendered}"))?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    ensure!(out.status.success(), "{rendered} failed: {}", stderr.trim());
    let text = String::from_utf8(out.stdout)
        .with_context(|| format!("{rendered} returned non-utf8"))?
        .trim()
        .to_owned();
    ensure!(!text.is_empty(), "{rendered} returned an empty identity");
    ensure!(
        !text.contains('\n'),
        "{rendered} returned more than one line"
    );
    Ok(text)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::judge::{Measurements, Verdict};
    use super::{Baseline, Machine, finish};

    fn sample() -> Baseline {
        Baseline::from_run(
            &Measurements::from([("cold_start_ms".to_owned(), 80.0)]),
            Machine {
                model: "Mac14,6".to_owned(),
                macos: "15.6".to_owned(),
            },
            "rev".to_owned(),
            "2026-11-01".to_owned(),
        )
    }

    #[test]
    fn baseline_round_trips_through_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("perf-baseline.json");
        let measurements = Measurements::from([
            ("cold_start_ms".to_owned(), 100.0),
            ("menu_open_cpu_ms".to_owned(), 20.0),
            ("modules_running".to_owned(), 1.0),
            ("submenu_open_ms".to_owned(), 9.0),
        ]);
        let saved = Baseline::from_run(
            &measurements,
            Machine {
                model: "Mac14,6".to_owned(),
                macos: "15.6.1".to_owned(),
            },
            "abc123".to_owned(),
            "2026-11-01".to_owned(),
        );
        saved.save(&path).unwrap();

        let raw: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw["machine"], serde_json::json!("Mac14,6"));
        assert_eq!(raw["macos"], serde_json::json!("15.6.1"));
        assert_eq!(raw["revision"], serde_json::json!("abc123"));
        assert_eq!(raw["recorded"], serde_json::json!("2026-11-01"));
        assert_eq!(
            raw["metrics"]["cold_start_ms"]["median"],
            serde_json::json!(100.0)
        );
        assert_eq!(
            raw["metrics"]["cold_start_ms"]["floor"],
            serde_json::json!(10.0)
        );
        assert_eq!(
            raw["metrics"]["menu_open_cpu_ms"]["median"],
            serde_json::json!(20.0)
        );
        assert_eq!(
            raw["metrics"]["menu_open_cpu_ms"]["floor"],
            serde_json::json!(5.0)
        );
        assert!(raw["metrics"].get("modules_running").is_none());
        assert!(raw["metrics"].get("submenu_open_ms").is_none());

        let loaded = Baseline::load(&path).unwrap().expect("loaded baseline");
        assert_eq!(loaded.machine.model, "Mac14,6");
        assert_eq!(loaded.machine.macos, "15.6.1");
        assert_eq!(loaded.revision, "abc123");
        assert_eq!(loaded.recorded, "2026-11-01");
        assert_eq!(loaded.metrics.get("cold_start_ms"), Some(&(100.0, 10.0)));
        assert_eq!(loaded.metrics.get("menu_open_cpu_ms"), Some(&(20.0, 5.0)));
        assert!(
            Baseline::load(&dir.path().join("absent.json"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn record_writes_only_when_verdict_passed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("perf-baseline.json");
        let original = b"{\"untouched\":true}\n".to_vec();
        fs::write(&path, &original).unwrap();
        let failed = Verdict {
            lines: Vec::new(),
            warnings: Vec::new(),
            passed: false,
        };
        let baseline = sample();
        let wrote = finish(&path, &failed, true, &baseline).unwrap();
        assert!(!wrote);
        assert_eq!(fs::read(&path).unwrap(), original);

        let passed = Verdict {
            lines: Vec::new(),
            warnings: Vec::new(),
            passed: true,
        };
        let fresh = dir.path().join("fresh.json");
        let wrote = finish(&fresh, &passed, true, &baseline).unwrap();
        assert!(wrote);
        let loaded = Baseline::load(&fresh).unwrap().expect("recorded baseline");
        assert_eq!(loaded.machine.model, "Mac14,6");
        assert_eq!(loaded.metrics.get("cold_start_ms"), Some(&(80.0, 10.0)));
    }

    #[test]
    fn without_record_never_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("perf-baseline.json");
        let passed = Verdict {
            lines: Vec::new(),
            warnings: Vec::new(),
            passed: true,
        };
        let wrote = finish(&path, &passed, false, &sample()).unwrap();
        assert!(!wrote);
        assert!(!path.exists());
    }

    #[test]
    fn current_machine_reads_model_and_product_version() {
        let machine = Machine::current().unwrap();
        assert!(!machine.model.is_empty(), "{}", machine.model);
        assert!(
            machine.macos.contains('.')
                && machine
                    .macos
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.'),
            "{}",
            machine.macos
        );
        assert!(
            !machine.model.chars().any(char::is_whitespace),
            "{}",
            machine.model
        );
    }
}
