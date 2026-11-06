#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Stdio};
use std::sync::Arc;

use navori_ffi::{
    AiDeps, CoreConfig, CoreSnapshot, NavoriCore, Origin, SnapshotListener, TurnEnd, TurnRequest,
};
use navori_secrets::SecretStore;
use navori_secrets::fake::{FakeKeychain, MemoryStore};
use tokio_util::sync::CancellationToken;

struct Quiet;

impl SnapshotListener for Quiet {
    fn on_snapshot(&self, _: CoreSnapshot) {}
}

fn required(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => value,
        _ => panic!("set {name} to run the live Agent loop test"),
    }
}

fn key_from_pass(entry: &str) -> String {
    let output = Command::new("pass")
        .arg("show")
        .arg(entry)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("could not run pass: {:?}", error.kind()));
    assert!(output.status.success(), "pass could not show the entry");
    let Ok(text) = String::from_utf8(output.stdout) else {
        panic!("the pass entry is not valid UTF-8");
    };
    text.trim().to_owned()
}

fn config(home: &tempfile::TempDir) -> CoreConfig {
    CoreConfig {
        profile: "dev".into(),
        home_dir: home.path().to_string_lossy().into_owned(),
        uid: std::os::unix::fs::MetadataExt::uid(&std::fs::metadata(home.path()).unwrap()),
        app_version: "0.1.0-live".into(),
        log_to_file: false,
        helper_path: home
            .path()
            .join("Navori Dev.app/Contents/Helpers/navori")
            .to_string_lossy()
            .into_owned(),
    }
}

fn actions_called(home: &tempfile::TempDir) -> Vec<String> {
    let data = home.path().join("Library/Application Support/Navori Dev");
    let store = navori_store::Store::open(&data.join("navori.db"), &data.join("backups")).unwrap();
    store.with_conn(|conn| {
        let mut statement = conn
            .prepare(
                "SELECT json_extract(call.value, '$.name') FROM ai_session_message m, json_each(m.body, '$.tool_calls') call WHERE m.kind = 'assistant' ORDER BY m.seq, call.key",
            )
            .unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    })
}

// Operator: NAVORI_LIVE_LLM=1 NAVORI_LIVE_PASS_ENTRY=<entry> NAVORI_LIVE_MODEL=<model> cargo test -p navori-ffi --test live_agent -- --ignored --nocapture
#[test]
#[ignore = "calls DeepSeek with a key from pass; run by hand with NAVORI_LIVE_LLM=1 and --ignored"]
fn a_live_turn_answers_from_a_real_read_action() {
    if std::env::var("NAVORI_LIVE_LLM").as_deref() != Ok("1") {
        println!("skipped: set NAVORI_LIVE_LLM=1 to call DeepSeek");
        return;
    }
    let entry = required("NAVORI_LIVE_PASS_ENTRY");
    let model = required("NAVORI_LIVE_MODEL");
    let key = key_from_pass(&entry);

    let home = tempfile::Builder::new()
        .prefix("nv")
        .tempdir_in("/tmp")
        .unwrap();
    let vault = FakeKeychain::new();
    let deps = AiDeps {
        open_secrets: Arc::new(move |service| {
            let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new(service, &vault));
            Ok(store)
        }),
        ..AiDeps::production()
    };
    let core = NavoriCore::start_with(config(&home), Arc::new(Quiet), deps).unwrap();
    futures::executor::block_on(core.save_provider_key(key)).unwrap();

    let request = TurnRequest {
        conversation: None,
        origin: Origin::User,
        model,
        input: "Which TCP ports are open on this Mac? Use the tools.".into(),
    };
    let outcome =
        futures::executor::block_on(core.run_agent_turn(request, CancellationToken::new()))
            .unwrap();

    println!("answer: {}", outcome.answer.as_deref().unwrap_or("<none>"));
    println!("end: {:?} after {} steps", outcome.end, outcome.steps);
    println!("actions called: {:?}", actions_called(&home));
    assert!(
        matches!(outcome.end, TurnEnd::Completed | TurnEnd::StepLimit),
        "the Turn ended with {:?}",
        outcome.end
    );
    core.shutdown();
}
