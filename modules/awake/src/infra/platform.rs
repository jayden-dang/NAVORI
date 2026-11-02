use std::sync::Arc;

use navori_macos::{
    ExitSink, ExitSource, KqueueExits, MacPower, MacProcesses, PlatformError, PowerAssertions,
    ProcessSource,
};

pub type ProcessFactory = Box<dyn FnMut() -> Result<Arc<dyn ProcessSource>, PlatformError> + Send>;
pub type ExitsFactory =
    Box<dyn FnMut(ExitSink) -> Result<Box<dyn ExitSource>, PlatformError> + Send>;

pub struct Platform {
    power: Arc<dyn PowerAssertions>,
    processes: ProcessFactory,
    exits: ExitsFactory,
}

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("power", &Arc::strong_count(&self.power))
            .finish_non_exhaustive()
    }
}

impl Platform {
    pub fn new(
        power: Arc<dyn PowerAssertions>,
        processes: ProcessFactory,
        exits: ExitsFactory,
    ) -> Self {
        Self {
            power,
            processes,
            exits,
        }
    }

    pub fn mac() -> Self {
        Self::new(
            Arc::new(MacPower),
            Box::new(|| Ok(Arc::new(MacProcesses::new()?) as Arc<dyn ProcessSource>)),
            Box::new(|sink| Ok(Box::new(KqueueExits::start(sink)?) as Box<dyn ExitSource>)),
        )
    }

    pub(crate) fn power(&self) -> Arc<dyn PowerAssertions> {
        Arc::clone(&self.power)
    }

    pub fn open_processes(&mut self) -> Result<Arc<dyn ProcessSource>, PlatformError> {
        (self.processes)()
    }

    pub fn open_exits(&mut self, sink: ExitSink) -> Result<Box<dyn ExitSource>, PlatformError> {
        (self.exits)(sink)
    }
}
