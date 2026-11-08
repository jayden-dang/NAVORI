//! Kernel: module registry, command routing and supervision.

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use futures::FutureExt;
use navori_protocol::{ActionSpec, Envelope, Event, ModuleInfo, ModuleState, Severity, TrayState};
use navori_store::{Store, StoreError};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::bus::{EventBus, now_ms};
use crate::dispatcher::{ModuleDispatcher, Route, Router, SharedRouter};
use crate::module::{ActionError, Module, ModuleAction, ModuleConfig, ModuleCtx, Reply, Request};
use crate::scheduler::{Scheduler, SystemClock, WallClock};
use crate::system::{SweepPolicy, System};
use crate::tray::{TrayContribution, compose_tray};

const INBOX_CAPACITY: usize = 256;
/// A module that ran this long before crashing gets its restart budget back.
const STABLE_AFTER: Duration = Duration::from_secs(60);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DispatchError {
    #[error("unknown module {0:?}")]
    UnknownModule(String),
    #[error("invalid action for {module}: {message}")]
    InvalidAction { module: String, message: String },
    #[error("module {0} is unavailable")]
    Unavailable(String),
    #[error("module {0} did not answer in time")]
    Timeout(String),
    #[error("refused destructive action {module}.{action}")]
    Refused { module: String, action: String },
    #[error(transparent)]
    Action(#[from] ActionError),
}

#[derive(Debug, Clone, Copy)]
pub struct RestartPolicy {
    /// Restarts allowed (since the last stable run) before the module is marked failed.
    pub max_restarts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            max_restarts: 5,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
        }
    }
}

pub(crate) type SendFn =
    Box<dyn Fn(Value) -> Result<oneshot::Receiver<Reply>, DispatchError> + Send + Sync>;
type StateFn = Box<dyn Fn() -> Value + Send + Sync>;
type TrayFn = Box<dyn Fn() -> TrayContribution + Send + Sync>;
type Launcher = Box<
    dyn FnOnce(
            RestartPolicy,
            &serde_json::Map<String, Value>,
            &mut Vec<&'static str>,
        ) -> JoinHandle<()>
        + Send,
>;

struct Registered {
    id: &'static str,
    specs: Vec<ActionSpec>,
    send: Option<SendFn>,
    state: StateFn,
    tray: TrayFn,
    info: Arc<Mutex<ModuleInfo>>,
}

/// Shared plumbing handed to every module at registration.
#[derive(Clone)]
struct Plumbing {
    store: Store,
    bus: EventBus,
    revision: Arc<watch::Sender<u64>>,
    shutdown: CancellationToken,
    scheduler: Scheduler,
    router: SharedRouter,
}

pub struct KernelBuilder {
    plumbing: Plumbing,
    modules: Vec<Registered>,
    launchers: Vec<Launcher>,
    policy: RestartPolicy,
    dispatch_timeout: Duration,
    sweep_policy: Arc<Mutex<SweepPolicy>>,
    sections: serde_json::Map<String, Value>,
    destructive_grants: Vec<&'static str>,
}

impl std::fmt::Debug for KernelBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<_> = self.modules.iter().map(|m| m.id).collect();
        f.debug_struct("KernelBuilder")
            .field("modules", &ids)
            .finish_non_exhaustive()
    }
}

impl KernelBuilder {
    pub fn restart_policy(mut self, policy: RestartPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Days of journal history the built-in `system` module keeps (minimum 1).
    pub fn journal_retention_days(self, days: u32) -> Self {
        *lock(&self.sweep_policy) = SweepPolicy::KeepDays(days.max(1));
        self
    }

    /// Leave the built-in journal sweep paused for this process.
    pub fn pause_journal_sweep(self) -> Self {
        *lock(&self.sweep_policy) = SweepPolicy::Paused;
        self
    }

    pub fn dispatch_timeout(mut self, timeout: Duration) -> Self {
        self.dispatch_timeout = timeout;
        self
    }

    pub fn config_sections(mut self, sections: serde_json::Map<String, Value>) -> Self {
        self.sections = sections;
        self
    }

    /// Allows `module` to dispatch catalogued `Destructive` Actions through its dispatcher.
    pub fn grant_destructive(mut self, module: &'static str) -> Self {
        self.destructive_grants.push(module);
        self
    }

    /// Registers a module. Module ids must be unique; a duplicate replaces nothing and is
    /// rejected loudly in debug builds.
    pub fn register<M: Module>(mut self, module: M) -> Self {
        debug_assert!(
            self.modules.iter().all(|m| m.id != M::ID),
            "duplicate module id {}",
            M::ID
        );

        let (inbox_tx, inbox_rx) = mpsc::channel::<Request<M::Action>>(INBOX_CAPACITY);
        let (state_tx, state_rx) = watch::channel(module.initial_state());
        let info = Arc::new(Mutex::new(ModuleInfo {
            id: M::ID.to_owned(),
            state: ModuleState::Starting,
            restarts: 0,
        }));

        let schema = serde_json::to_value(schemars::schema_for!(M::Action)).unwrap_or(Value::Null);
        let specs = M::Action::catalog()
            .into_iter()
            .map(|d| ActionSpec {
                module: M::ID.to_owned(),
                name: d.name.to_owned(),
                description: d.description.to_owned(),
                risk: d.risk,
                input_schema: schema.clone(),
            })
            .collect();

        let send: SendFn = Box::new(move |raw: Value| {
            let action: M::Action =
                serde_json::from_value(raw).map_err(|e| DispatchError::InvalidAction {
                    module: M::ID.to_owned(),
                    message: e.to_string(),
                })?;
            let (req, rx) = Request::new(action);
            inbox_tx
                .try_send(req)
                .map_err(|_| DispatchError::Unavailable(M::ID.to_owned()))?;
            Ok(rx)
        });
        let tray_rx = state_rx.clone();
        let state: StateFn =
            Box::new(move || serde_json::to_value(&*state_rx.borrow()).unwrap_or(Value::Null));
        let tray_broken = AtomicBool::new(false);
        let tray: TrayFn = Box::new(move || {
            if tray_broken.load(Ordering::Relaxed) {
                return TrayContribution::default();
            }
            // Clone out of the borrow so `M::tray` never runs under the watch lock.
            let current = tray_rx.borrow().clone();
            // A panic in Module code must not take down a status read; the Module then
            // asks for nothing, and is not called again.
            catch_unwind(AssertUnwindSafe(|| M::tray(&current))).unwrap_or_else(|panic| {
                tray_broken.store(true, Ordering::Relaxed);
                tracing::warn!(
                    module = M::ID,
                    panic = %panic_message(&*panic),
                    "module tray panicked; ignoring it from now on"
                );
                TrayContribution::default()
            })
        });

        let sup_info = info.clone();
        let plumbing = self.plumbing.clone();
        self.launchers
            .push(Box::new(move |policy, sections, issues| {
                let config = module_config::<M>(sections, issues);
                let _ = state_tx.send(module.initial_state());
                let ctx = ModuleCtx::<M> {
                    inbox: inbox_rx,
                    state: state_tx,
                    revision: plumbing.revision.clone(),
                    bus: plumbing.bus.clone(),
                    store: plumbing.store.clone(),
                    shutdown: plumbing.shutdown.clone(),
                    scheduler: plumbing.scheduler.clone(),
                    config,
                    dispatcher: ModuleDispatcher {
                        caller: M::ID,
                        router: plumbing.router.clone(),
                    },
                };
                tokio::spawn(supervise(module, ctx, sup_info, policy, plumbing))
            }));
        self.modules.push(Registered {
            id: M::ID,
            specs,
            send: Some(send),
            state,
            tray,
            info,
        });
        self
    }

    /// Spawns every module on the current tokio runtime.
    pub fn start(mut self) -> Kernel {
        let registered_ids: HashSet<&'static str> = self.modules.iter().map(|m| m.id).collect();
        let grants: HashSet<&'static str> = self
            .destructive_grants
            .into_iter()
            .filter(|id| {
                if registered_ids.contains(id) {
                    true
                } else {
                    tracing::error!(
                        module = %id,
                        "destructive grant names no registered module; dropping"
                    );
                    false
                }
            })
            .collect();

        let mut routes = Vec::with_capacity(self.modules.len());
        for m in &mut self.modules {
            let Some(send) = m.send.take() else {
                unreachable!("each registered module has a send fn once");
            };
            routes.push(Route {
                id: m.id,
                send,
                specs: m.specs.clone(),
            });
        }
        if self
            .plumbing
            .router
            .set(Router {
                routes,
                grants,
                timeout: self.dispatch_timeout,
            })
            .is_err()
        {
            unreachable!("router is set only once at start");
        }

        let sections = std::mem::take(&mut self.sections);
        let mut config_issues = Vec::new();
        let tasks = std::mem::take(&mut self.launchers)
            .into_iter()
            .map(|launch| launch(self.policy, &sections, &mut config_issues))
            .collect();
        Kernel {
            modules: self.modules,
            plumbing: self.plumbing,
            dispatch_timeout: self.dispatch_timeout,
            tasks: Mutex::new(tasks),
            started_at_ms: now_ms(),
            started: Instant::now(),
            config_issues,
        }
    }
}

fn module_config<M: Module>(
    sections: &serde_json::Map<String, Value>,
    issues: &mut Vec<&'static str>,
) -> Arc<M::Config> {
    let Some(value) = sections.get(M::ID) else {
        return Arc::new(M::Config::default());
    };
    let Ok(config) = M::Config::deserialize(value) else {
        issues.push(M::ID);
        return Arc::new(M::Config::default());
    };
    if config.check().is_err() {
        issues.push(M::ID);
        return Arc::new(M::Config::default());
    }
    Arc::new(config)
}

pub struct Kernel {
    modules: Vec<Registered>,
    plumbing: Plumbing,
    dispatch_timeout: Duration,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    started_at_ms: i64,
    started: Instant,
    config_issues: Vec<&'static str>,
}

impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kernel")
            .field("modules", &self.modules())
            .finish_non_exhaustive()
    }
}

impl Kernel {
    /// A builder with the built-in `system` module already registered.
    pub fn builder(store: Store) -> KernelBuilder {
        Self::builder_with_clock(store, Arc::new(SystemClock))
    }

    pub fn builder_with_clock(store: Store, clock: Arc<dyn WallClock>) -> KernelBuilder {
        let (revision, _) = watch::channel(0);
        let plumbing = Plumbing {
            bus: EventBus::new(store.clone()),
            store,
            revision: Arc::new(revision),
            shutdown: CancellationToken::new(),
            scheduler: Scheduler::new(clock),
            router: Arc::new(OnceLock::new()),
        };
        let system = System::default();
        KernelBuilder {
            plumbing,
            modules: Vec::new(),
            launchers: Vec::new(),
            policy: RestartPolicy::default(),
            dispatch_timeout: Duration::from_secs(30),
            sweep_policy: Arc::clone(&system.sweep_policy),
            sections: serde_json::Map::new(),
            destructive_grants: Vec::new(),
        }
        .register(system)
    }

    pub fn config_issues(&self) -> &[&'static str] {
        &self.config_issues
    }

    pub fn scheduler(&self) -> &Scheduler {
        &self.plumbing.scheduler
    }

    pub fn actions(&self) -> Vec<ActionSpec> {
        let Some(router) = self.plumbing.router.get() else {
            unreachable!("router is filled at start");
        };
        router
            .routes
            .iter()
            .flat_map(|r| r.specs.iter().cloned())
            .collect()
    }

    pub async fn dispatch(&self, envelope: Envelope) -> Result<Value, DispatchError> {
        let Some(router) = self.plumbing.router.get() else {
            unreachable!("router is filled at start");
        };
        let route = router
            .get(envelope.module.as_str())
            .ok_or_else(|| DispatchError::UnknownModule(envelope.module.clone()))?;
        let module = envelope.module.clone();
        let rx = (route.send)(envelope.action)?;
        match tokio::time::timeout(self.dispatch_timeout, rx).await {
            Err(_) => Err(DispatchError::Timeout(module)),
            Ok(Err(_)) => Err(DispatchError::Unavailable(module)),
            Ok(Ok(reply)) => reply.map_err(DispatchError::from),
        }
    }

    pub fn modules(&self) -> Vec<ModuleInfo> {
        self.modules.iter().map(|m| lock(&m.info).clone()).collect()
    }

    /// The status icon state: Module lifecycle and contributions, plus the config notice as
    /// `(title, message)`.
    pub fn tray(&self, notice: Option<(&str, &str)>) -> TrayState {
        let entries: Vec<_> = self
            .modules
            .iter()
            .map(|m| (lock(&m.info).clone(), (m.tray)()))
            .collect();
        compose_tray(&entries, notice)
    }

    /// Current state snapshot of `module` as JSON.
    pub fn state(&self, module: &str) -> Option<Value> {
        self.modules
            .iter()
            .find(|m| m.id == module)
            .map(|m| (m.state)())
    }

    /// Increments whenever any module state or module lifecycle changes.
    pub fn revision(&self) -> watch::Receiver<u64> {
        self.plumbing.revision.subscribe()
    }

    pub fn bus(&self) -> &EventBus {
        &self.plumbing.bus
    }

    pub fn store(&self) -> &Store {
        &self.plumbing.store
    }

    pub fn recent_events(&self, limit: usize) -> Result<Vec<Event>, StoreError> {
        self.plumbing.store.journal_recent(limit)
    }

    pub fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }

    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    /// Cancels every module and waits (bounded) for them to stop. Idempotent.
    pub async fn shutdown(&self) {
        self.plumbing.shutdown.cancel();
        let tasks: Vec<_> = std::mem::take(&mut *lock(&self.tasks));
        let aborts: Vec<_> = tasks.iter().map(JoinHandle::abort_handle).collect();
        if tokio::time::timeout(SHUTDOWN_GRACE, futures::future::join_all(tasks))
            .await
            .is_err()
        {
            tracing::warn!("modules did not stop within {SHUTDOWN_GRACE:?}; aborting");
            aborts.iter().for_each(tokio::task::AbortHandle::abort);
        }
        for m in &self.modules {
            set_state(&m.info, &self.plumbing.revision, ModuleState::Stopped);
        }
    }
}

impl Drop for Kernel {
    fn drop(&mut self) {
        // Router keeps inbox senders alive in an Arc the module tasks also hold, so
        // dropping the Kernel alone no longer closes those inboxes. Cancel so recv
        // returns None the way a closed inbox used to.
        self.plumbing.shutdown.cancel();
    }
}

async fn supervise<M: Module>(
    mut module: M,
    mut ctx: ModuleCtx<M>,
    info: Arc<Mutex<ModuleInfo>>,
    policy: RestartPolicy,
    plumbing: Plumbing,
) {
    let mut attempt: u32 = 0;
    loop {
        set_state(&info, &plumbing.revision, ModuleState::Running);
        let started = Instant::now();
        let outcome = AssertUnwindSafe(module.run(&mut ctx)).catch_unwind().await;

        if plumbing.shutdown.is_cancelled() {
            break;
        }
        let error = match outcome {
            Ok(Ok(())) => break,
            Ok(Err(e)) => format!("{e:#}"),
            Err(panic) => panic_message(&*panic),
        };
        if started.elapsed() >= STABLE_AFTER {
            attempt = 0;
        }

        if attempt >= policy.max_restarts {
            tracing::error!(module = M::ID, %error, "module failed permanently");
            set_state(
                &info,
                &plumbing.revision,
                ModuleState::Failed {
                    last_error: error.clone(),
                },
            );
            plumbing.bus.emit(
                M::ID,
                "module.failed",
                Severity::Error,
                format!("{} stopped after repeated crashes: {error}", M::ID),
                json!({ "error": error }),
            );
            // Dropping ctx closes the inbox: callers now get `Unavailable`.
            return;
        }

        attempt += 1;
        tracing::warn!(module = M::ID, attempt, %error, "module crashed; restarting");
        lock(&info).restarts += 1;
        set_state(
            &info,
            &plumbing.revision,
            ModuleState::Restarting {
                attempt,
                last_error: error.clone(),
            },
        );
        plumbing.bus.emit(
            M::ID,
            "module.crashed",
            Severity::Warn,
            format!("{} crashed and is restarting: {error}", M::ID),
            json!({ "error": error, "attempt": attempt }),
        );

        let delay = policy
            .base_delay
            .saturating_mul(1 << (attempt - 1).min(16))
            .min(policy.max_delay);
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = plumbing.shutdown.cancelled() => break,
        }
    }
    set_state(&info, &plumbing.revision, ModuleState::Stopped);
}

fn set_state(info: &Mutex<ModuleInfo>, revision: &watch::Sender<u64>, state: ModuleState) {
    lock(info).state = state;
    revision.send_modify(|r| *r += 1);
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        format!("panic: {s}")
    } else if let Some(s) = panic.downcast_ref::<String>() {
        format!("panic: {s}")
    } else {
        "panic".to_owned()
    }
}
