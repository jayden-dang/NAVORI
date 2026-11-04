//! Stopping what holds a port.

mod process;
mod resolve;
mod task;

pub use process::{member_outcome, reach_for, stop_process};

pub(crate) use task::{StopDone, StopEnv, StopTask, Stops};

pub use resolve::{
    ContainerPlan, FreshRead, Guard, Plan, ProcessPlan, Reach, Resolved, StopClass, resolve,
    stop_class,
};

/// The fixed sentences a stop answers with.
pub mod reason {
    pub const PROCESS_CHANGED: &str = "The process changed";
    pub const PORT_CLOSED: &str = "The port closed";
    pub const CONTAINER_NOT_RUNNING: &str = "The container is not running";
    pub const PROCESS_PROTECTED: &str = "This process is protected";
    pub const NOT_STOPPED_WITH_GROUP: &str = "This port is not stopped with a group";
    pub const NOT_IN_PROJECT: &str = "It is not in that project";
    pub const DOCKER_UNREACHABLE: &str = "Docker is not reachable";
    pub const SIGNAL_REFUSED: &str = "The signal was refused";
    pub const STILL_RUNNING: &str = "It is still running";
    pub const PROCESS_LIST_TIMEOUT: &str = "The process list could not be read in time";
    pub const DOCKER_REFUSED: &str = "Docker refused the stop";
    pub const GUARD_INCOMPLETE: &str = "Navori could not check what it must not stop";
    pub const CONTAINER_ID_REFUSED: &str = "Navori did not send that container id to Docker";
    pub const PROCESS_LIST_UNREADABLE: &str = "Navori could not read the process list";
    pub const TARGET_UNREAD_IN_TIME: &str = "The target could not be read in time";
    pub const STILL_STOPPING: &str = "It is still stopping";
}
