use semver::Version;

/// We parse the crate version at runtime,
/// so we should make sure that parse won't fail for the artifact this build produces
#[test]
fn crate_version_is_semver() {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
}
