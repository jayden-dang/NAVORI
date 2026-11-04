use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::Path;

use crate::infra::docker::Container;
use crate::kind::DOCKER_BACKEND;
use crate::scan::BackendPort;
use crate::state::{ContainerRef, Placement, PortEntry, PortKind};

/// Gives each Docker backend port its containers, one entry per (process, port, container). A bound
/// address no container published stays one entry of its own with no project.
pub fn attribute(
    backend: &[BackendPort],
    containers: &[Container],
    place: impl Fn(&Path) -> Placement,
) -> Vec<PortEntry> {
    let mut entries = Vec::new();
    for port in backend {
        let mut unclaimed: BTreeSet<IpAddr> = port.addresses.iter().copied().collect();
        for container in containers {
            let claimed: BTreeSet<IpAddr> = port
                .addresses
                .iter()
                .copied()
                .filter(|bound| publishes(container, port.port, *bound))
                .collect();
            if claimed.is_empty() {
                continue;
            }
            unclaimed.retain(|bound| !claimed.contains(bound));
            let placed = container
                .working_dir
                .as_deref()
                .filter(|dir| dir.is_absolute())
                .map(&place)
                .unwrap_or_default();
            let held = ContainerRef {
                id: container.id.clone(),
                name: container.name.clone(),
                image: container.image.clone(),
            };
            entries.push(entry(port, claimed, placed, Some(held)));
        }
        if !unclaimed.is_empty() {
            entries.push(entry(port, unclaimed, Placement::default(), None));
        }
    }
    entries
}

fn publishes(container: &Container, port: u16, bound: IpAddr) -> bool {
    container.ports.iter().any(|(published, public)| {
        *public == port
            && (*published == bound || (published.is_unspecified() && bound.is_unspecified()))
    })
}

fn entry(
    port: &BackendPort,
    addresses: BTreeSet<IpAddr>,
    place: Placement,
    container: Option<ContainerRef>,
) -> PortEntry {
    PortEntry {
        port: port.port,
        addresses: addresses.into_iter().collect(),
        pid: port.pid,
        start_us: port.start_us,
        name: DOCKER_BACKEND.to_owned(),
        exe: port.exe.clone(),
        place,
        harness: None,
        kind: PortKind::Docker,
        container,
    }
}
