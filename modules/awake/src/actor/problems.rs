use std::collections::BTreeMap;
use std::time::Duration;

pub(super) const MAX_SECONDS: u64 = 7 * 24 * 60 * 60;
pub(super) const RETRY: Duration = Duration::from_secs(60);
pub(crate) const SYSTEM_PROBLEM: &str = "macOS refused the request. Navori keeps trying.";
pub(super) const DISPLAY_PROBLEM: &str =
    "macOS refused to keep the display on. Navori keeps trying.";
pub(super) const RELEASE_PROBLEM: &str = "Releasing a power assertion failed.";
pub(super) const EXITS_PROBLEM: &str =
    "Exit watches stopped, so a command lease has no end signal. Navori retries in 60 seconds.";
pub(super) const LID_READ_PROBLEM: &str =
    "Awake could not read the Lid closed setting, so it reads as off.";
pub(super) const POWER_PROBLEM: &str = "Awake could not watch the power source, so it reads as on battery. Navori retries in 60 seconds.";
pub(super) const LID_MARKER_PROBLEM: &str = "Awake could not save its lid marker, so it did not hold the lid. Navori retries in 60 seconds.";
pub(super) const LID_MARKER_READ_PROBLEM: &str =
    "Awake could not read its lid marker, so it did not give the lid back.";
pub(super) const LID_REFUSED_PROBLEM: &str =
    "macOS refused to keep the lid awake, so Lid closed is off.";
pub(super) const LID_CLEAR_PROBLEM: &str =
    "Awake could not give the lid setting back. Navori retries in 60 seconds.";
pub(super) const LAST_SLEEP_PROBLEM: &str = "Awake could not save when the Mac last slept.";
pub(super) const LAST_SLEEP_READ_PROBLEM: &str = "Awake could not read when the Mac last slept, so the Last sleep row is hidden. Navori keeps working.";
pub(super) const LID_SAVE_OFF_PROBLEM: &str =
    "Awake could not save the Lid closed setting as off, so it may come back on after a restart.";
pub(super) const STORE_PROBLEM: &str =
    "Awake could not save a lease. Navori retries in 60 seconds.";
pub(super) const PLATFORM_PROBLEM: &str = "Awake could not watch a process, so a stored lease stays on disk. Navori retries in 60 seconds.";

#[derive(Debug, Default)]
pub(super) struct Problems {
    pub(super) system: Option<String>,
    pub(super) display: Option<String>,
    pub(super) release: Option<String>,
    pub(super) store: Option<String>,
    pub(super) platform: Option<String>,
    pub(super) exits: Option<String>,
}

impl Problems {
    pub(super) fn first(&self) -> Option<String> {
        self.system
            .clone()
            .or_else(|| self.display.clone())
            .or_else(|| self.release.clone())
            .or_else(|| self.store.clone())
            .or_else(|| self.platform.clone())
            .or_else(|| self.exits.clone())
    }
}

/// Declaration order is severity: the first kind that is set is the one published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum LidProblemKind {
    Clear,
    MarkerRead,
    Marker,
    SaveOff,
    Refused,
    Power,
    LidRead,
    LastSleepRead,
    LastSleep,
}

#[derive(Debug, Default)]
pub(super) struct LidProblems(BTreeMap<LidProblemKind, &'static str>);

impl LidProblems {
    pub(super) fn raise(&mut self, kind: LidProblemKind, text: &'static str) {
        self.0.insert(kind, text);
    }

    pub(super) fn clear(&mut self, kind: LidProblemKind) {
        self.0.remove(&kind);
    }

    pub(super) fn clear_all_but_give_back(&mut self) {
        self.0.retain(|kind, _| *kind == LidProblemKind::Clear);
    }

    pub(super) fn first(&self) -> Option<String> {
        self.0.values().next().map(|text| (*text).to_owned())
    }
}
