//! ref: composer/tests/Composer/Test/Repository/ArtifactRepositoryTest.php sha1:2f0f2b004082db7001b8e29d32d177289b1e19dc

use bstr::{BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe::io::{IOInterface, NullIO};
use shirabe::repository::ArtifactRepository;
use shirabe_php_shim::{PhpMixed, bformat, extension_loaded, path_to_bytes, str_replace};

/// Returns true when the test should be skipped because the zip extension is
/// unavailable, mirroring PHP's markTestSkipped in setUp.
fn set_up() -> bool {
    if !extension_loaded("zip") {
        // markTestSkipped('You need the zip extension to run this test.')
        return true;
    }
    false
}

fn artifacts_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Repository/Fixtures/artifacts")
}

fn create_repo(url: impl AsRef<[u8]>) -> ArtifactRepository {
    let mut coordinates: IndexMap<BString, PhpMixed> = IndexMap::new();
    coordinates.insert("type".into(), PhpMixed::string(BString::from("artifact")));
    coordinates.insert("url".into(), PhpMixed::string(url.as_ref()));

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    ArtifactRepository::new(coordinates, io).unwrap()
}

#[test]
fn test_extracts_configs_from_zip_archives() {
    if set_up() {
        return;
    }

    let mut expected_packages = vec![
        BString::from("vendor0/package0-0.0.1"),
        BString::from("composer/composer-1.0.0-alpha6"),
        BString::from("vendor1/package2-4.3.2"),
        BString::from("vendor3/package1-5.4.3"),
        BString::from("test/jsonInRoot-1.0.0"),
        BString::from("test/jsonInRootTarFile-1.0.0"),
        BString::from("test/jsonInFirstLevel-1.0.0"),
        // The files not-an-artifact.zip and jsonSecondLevel are not valid
        // artifacts and do not get detected.
    ];

    let mut repo = create_repo(path_to_bytes(&artifacts_dir()));

    let mut found_packages: Vec<bstr::BString> = repo
        .__get_packages()
        .unwrap()
        .iter()
        .map(|package| {
            bformat!(
                "{}-{}",
                package.get_pretty_name(),
                package.get_pretty_version()
            )
        })
        .collect();

    expected_packages.sort();
    found_packages.sort();

    assert_eq!(expected_packages, found_packages);

    let tar_package: Vec<_> = repo
        .__get_packages()
        .unwrap()
        .into_iter()
        .filter(|package| package.get_pretty_name() == "test/jsonInRootTarFile")
        .collect();
    assert_eq!(1, tar_package.len());
    let tar_package = tar_package.into_iter().next_back().unwrap();
    assert_eq!(
        Some(bstr::BString::from("tar")),
        tar_package.get_dist_type()
    );
}

#[test]
fn test_absolute_repo_url_creates_absolute_url_packages() {
    if set_up() {
        return;
    }

    let absolute_path = artifacts_dir();
    let mut repo = create_repo(path_to_bytes(&absolute_path));

    for package in repo.__get_packages().unwrap() {
        assert_eq!(
            package.get_dist_url().unwrap_or_default().find(str_replace(
                "\\",
                "/",
                path_to_bytes(&absolute_path)
            )),
            Some(0)
        );
    }
}

#[test]
#[ignore = "the relative url is resolved from the process cwd (the crate manifest dir under cargo, not the composer test root), so the artifacts dir is not found"]
fn test_relative_repo_url_creates_relative_url_packages() {
    if set_up() {
        return;
    }

    let relative_path = "tests/Composer/Test/Repository/Fixtures/artifacts";
    let mut repo = create_repo(relative_path);

    for package in repo.__get_packages().unwrap() {
        assert_eq!(
            package
                .get_dist_url()
                .unwrap_or_default()
                .find(relative_path),
            Some(0)
        );
    }
}
