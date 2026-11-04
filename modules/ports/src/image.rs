const DATABASES: [&str; 6] = ["postgres", "mysql", "mariadb", "redis", "valkey", "mongo"];

/// The last path segment of an image reference, without registry, namespace, tag or digest;
/// `None` for a bare image id.
pub(crate) fn image_name(reference: &str) -> Option<String> {
    if reference.starts_with("sha256:") {
        return None;
    }
    let without_digest = reference.split('@').next().unwrap_or_default();
    let last = without_digest.rsplit('/').next().unwrap_or_default();
    let name = last.split(':').next().unwrap_or_default();
    (!name.is_empty()).then(|| name.to_owned())
}

/// Whether the image is one of the well-known databases, by its last path segment.
pub fn database_image(image: &str) -> bool {
    let name = image.rsplit('/').next().unwrap_or_default();
    DATABASES.contains(&name)
}
