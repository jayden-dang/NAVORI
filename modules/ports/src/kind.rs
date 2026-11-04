use std::path::Path;

use crate::state::PortKind;

pub(crate) const DOCKER_BACKEND: &str = "com.docker.backend";
const DATABASES: [&str; 6] = [
    "postgres",
    "mysqld",
    "mariadbd",
    "redis-server",
    "valkey-server",
    "mongod",
];
const SYSTEM_PATHS: [&str; 3] = ["/System/", "/usr/libexec/", "/usr/sbin/"];
const NOT_SYSTEM: &str = "/System/Volumes/Data/";

/// The kind of thing holding a port, from its executable and whether it runs under a project.
pub fn kind(exe: Option<&Path>, placed: bool) -> PortKind {
    let name = exe.and_then(Path::file_name);
    if name.is_some_and(|name| name == DOCKER_BACKEND) {
        return PortKind::Docker;
    }
    if name.is_some_and(|name| DATABASES.iter().any(|database| name == *database)) {
        return PortKind::Database;
    }
    if !placed && exe.is_some_and(is_system) {
        return PortKind::System;
    }
    if placed {
        PortKind::DevServer
    } else {
        PortKind::Other
    }
}

/// Whether the executable is the Docker backend, whose listeners belong to containers.
pub fn is_docker_backend(exe: Option<&Path>) -> bool {
    exe.and_then(Path::file_name)
        .is_some_and(|name| name == DOCKER_BACKEND)
}

fn is_system(exe: &Path) -> bool {
    !exe.starts_with(NOT_SYSTEM) && SYSTEM_PATHS.iter().any(|prefix| exe.starts_with(prefix))
}
