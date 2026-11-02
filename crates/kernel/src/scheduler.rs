use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::watch;

pub trait WallClock: Send + Sync {
    fn now(&self) -> SystemTime;
}

#[derive(Debug)]
pub struct SystemClock;

impl WallClock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

#[derive(Debug)]
struct Inputs {
    panel_visible: bool,
    panel_changes: u64,
    wake_pulses: u64,
}

#[derive(Clone)]
pub struct Scheduler {
    inputs: Arc<watch::Sender<Inputs>>,
    clock: Arc<dyn WallClock>,
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scheduler")
            .field("panel_visible", &self.panel_visible())
            .finish_non_exhaustive()
    }
}

impl Scheduler {
    pub(crate) fn new(clock: Arc<dyn WallClock>) -> Self {
        let (inputs, _) = watch::channel(Inputs {
            panel_visible: false,
            panel_changes: 0,
            wake_pulses: 0,
        });
        Self {
            inputs: Arc::new(inputs),
            clock,
        }
    }

    pub fn wait_handle(&self) -> WaitHandle {
        let inputs = self.inputs.subscribe();
        let (seen_panel_changes, seen_wake_pulses) = {
            let current = inputs.borrow();
            (current.panel_changes, current.wake_pulses)
        };
        WaitHandle {
            scheduler: self.clone(),
            inputs,
            seen_panel_changes,
            seen_wake_pulses,
            last_timer_wake: None,
        }
    }

    pub fn set_panel_visible(&self, visible: bool) {
        let _ = self.inputs.send_if_modified(|inputs| {
            if inputs.panel_visible == visible {
                return false;
            }
            inputs.panel_visible = visible;
            inputs.panel_changes += 1;
            true
        });
    }

    pub fn panel_visible(&self) -> bool {
        self.inputs.borrow().panel_visible
    }

    pub fn wake_pulse(&self) {
        self.inputs.send_modify(|inputs| {
            inputs.wake_pulses += 1;
        });
    }
}

const CLOSED_MENU_TIMER_GAP: Duration = Duration::from_secs(30);

pub struct WaitHandle {
    scheduler: Scheduler,
    inputs: watch::Receiver<Inputs>,
    seen_panel_changes: u64,
    seen_wake_pulses: u64,
    last_timer_wake: Option<tokio::time::Instant>,
}

impl fmt::Debug for WaitHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WaitHandle")
            .field("seen_panel_changes", &self.seen_panel_changes)
            .field("seen_wake_pulses", &self.seen_wake_pulses)
            .field("last_timer_wake", &self.last_timer_wake)
            .finish_non_exhaustive()
    }
}

impl WaitHandle {
    pub async fn wait_until(&mut self, deadline: Option<SystemTime>) -> Wake {
        loop {
            let Some(deadline) = deadline else {
                if self.inputs.changed().await.is_err() {
                    std::future::pending::<()>().await;
                }
                continue;
            };

            let remaining = deadline
                .duration_since(self.now())
                .unwrap_or(Duration::ZERO);
            let park = remaining.max(self.closed_menu_gap());
            tokio::select! {
                biased;
                changed = self.inputs.changed() => {
                    if changed.is_err() {
                        std::future::pending::<()>().await;
                    }
                }
                () = tokio::time::sleep(park) => {
                    if self.now() >= deadline {
                        self.last_timer_wake = Some(tokio::time::Instant::now());
                        return Wake::Due;
                    }
                }
            }
        }
    }

    fn closed_menu_gap(&self) -> Duration {
        if self.panel_visible() {
            return Duration::ZERO;
        }
        let Some(last) = self.last_timer_wake else {
            return Duration::ZERO;
        };
        CLOSED_MENU_TIMER_GAP.saturating_sub(last.elapsed())
    }

    pub fn panel_visible(&self) -> bool {
        self.inputs.borrow().panel_visible
    }

    pub fn now(&self) -> SystemTime {
        self.scheduler.clock.now()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wake {
    Due,
    PanelChanged,
}
