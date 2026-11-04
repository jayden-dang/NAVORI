use navori_mod_ports::database_image;

#[test]
fn well_known_database_images_are_recognised_by_their_last_path_segment() {
    for image in [
        "postgres",
        "library/postgres",
        "docker.io/library/redis",
        "mongo",
    ] {
        assert!(database_image(image), "{image}");
    }
}

#[test]
fn look_alike_and_unrelated_images_are_not_databases() {
    for image in ["postgres-exporter", "bitnami/postgresql", "minio/minio", ""] {
        assert!(!database_image(image), "{image}");
    }
}
