const DATABASES: [&str; 6] = ["postgres", "mysql", "mariadb", "redis", "valkey", "mongo"];

/// The repository of an image reference, without its tag or digest; `None` for a bare image id.
pub(crate) fn repository(reference: &str) -> Option<String> {
    if reference.starts_with("sha256:") {
        return None;
    }
    let without_digest = reference.split('@').next().unwrap_or_default();
    let name_start = without_digest.rfind('/').map_or(0, |slash| slash + 1);
    let (head, last) = without_digest.split_at(name_start);
    let name = last.split(':').next().unwrap_or_default();
    let repository = format!("{head}{name}");
    (!name.is_empty()).then_some(repository)
}

/// Whether the image is one of the well-known databases, by its last path segment.
pub fn database_image(repository: &str) -> bool {
    let name = repository.rsplit('/').next().unwrap_or_default();
    DATABASES.contains(&name)
}
