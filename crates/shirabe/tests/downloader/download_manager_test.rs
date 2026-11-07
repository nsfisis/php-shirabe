//! ref: composer/tests/Composer/Test/Downloader/DownloadManagerTest.php sha1:beed3b24639e01796f78c002a129c1d4d65fb5d4

use crate::async_runtime::run;
use crate::io_stub::IOStub;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::downloader::{DownloadManager, DownloaderInterface};
use shirabe::io::IOInterface;
use shirabe::package::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe_php_shim::{PhpMixed, RuntimeException, bformat, impl_php_class, php_str};
use shirabe_semver::VersionParser;

// PHP mocks `Composer\Downloader\DownloaderInterface` with getMockBuilder.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_downloader {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub Downloader {}
        #[async_trait::async_trait(?Send)]
        impl DownloaderInterface for Downloader {
            fn get_installation_source(&self) -> bstr::BString;
            async fn download_with_output(
                &self,
                package: PackageInterfaceHandle,
                path: &std::path::Path,
                prev_package: Option<PackageInterfaceHandle>,
                output: bool,
            ) -> anyhow::Result<Option<PhpMixed>>;
            async fn prepare(
                &self,
                r#type: &bstr::BStr,
                package: PackageInterfaceHandle,
                path: &std::path::Path,
                prev_package: Option<PackageInterfaceHandle>,
            ) -> anyhow::Result<Option<PhpMixed>>;
            async fn install_with_output(
                &self,
                package: PackageInterfaceHandle,
                path: &std::path::Path,
                output: bool,
            ) -> anyhow::Result<Option<PhpMixed>>;
            async fn update(
                &self,
                initial: PackageInterfaceHandle,
                target: PackageInterfaceHandle,
                path: &std::path::Path,
            ) -> anyhow::Result<Option<PhpMixed>>;
            async fn remove_with_output(
                &self,
                package: PackageInterfaceHandle,
                path: &std::path::Path,
                output: bool,
            ) -> anyhow::Result<Option<PhpMixed>>;
            async fn cleanup(
                &self,
                r#type: &bstr::BStr,
                package: PackageInterfaceHandle,
                path: &std::path::Path,
                prev_package: Option<PackageInterfaceHandle>,
            ) -> anyhow::Result<Option<PhpMixed>>;
        }
    }
}
use mock_downloader::MockDownloader;

// PHPUnit reports a generated class name for `getMockBuilder(DownloaderInterface::class)`;
// no ported assertion reads it, so the mock answers with the interface it stands in for.
impl_php_class!(MockDownloader, r"Composer\Downloader\DownloaderInterface");

/// ref: DownloadManagerTest::createPackageMock
///
/// PHPUnit returns a `PackageInterface` mock; a real CompletePackage with the
/// relevant fields left at their defaults is an equivalent stand-in for the
/// installation-source/type dispatch logic exercised by the ported cases.
fn create_package_mock() -> PackageInterfaceHandle {
    make_package(BStr::new("dummy/pkg"), false)
}

/// Real package stand-in whose dev flag is derived from the version stability, so
/// `isDev()` can be controlled (`dev-master` => dev, `1.0.0` => stable).
fn make_package(name: &BStr, is_dev: bool) -> PackageInterfaceHandle {
    let (version, pretty) = if is_dev {
        (
            bstr::BString::from("dev-master"),
            BString::from("dev-master"),
        )
    } else {
        (
            VersionParser.normalize("1.0.0", None).unwrap(),
            BString::from("1.0.0"),
        )
    };
    CompletePackageHandle::new(bformat!("{name}"), version, pretty).into()
}

/// ref: DownloadManagerTest::createDownloaderMock
fn create_downloader_mock() -> std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>> {
    as_dyn(MockDownloader::new())
}

/// A `createDownloaderMock()` whose `getInstallationSource()` reports the given
/// source, matching the type under which it is registered with the manager so the
/// real `getDownloaderForPackage` dispatch resolves to it.
fn downloader_mock(installation_source: &BStr) -> MockDownloader {
    let mut downloader = MockDownloader::new();
    let source = bstr::BString::from(installation_source);
    downloader
        .expect_get_installation_source()
        .returning(move || source.clone());
    downloader
}

fn as_dyn(downloader: MockDownloader) -> std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>> {
    std::rc::Rc::new(std::cell::RefCell::new(downloader))
        as std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>
}

fn create_manager() -> DownloadManager {
    let io = std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()))
        as std::rc::Rc<std::cell::RefCell<dyn IOInterface>>;
    DownloadManager::new(io, false, None)
}

#[test]
fn test_set_get_downloader() {
    let downloader = create_downloader_mock();
    let mut manager = create_manager();

    manager.set_downloader("test", downloader.clone());
    assert!(std::rc::Rc::ptr_eq(
        &downloader,
        &manager.get_downloader("test").unwrap()
    ));

    let result = manager.get_downloader("unregistered");
    assert!(result.is_err());
}

#[test]
fn test_get_downloader_for_incorrectly_installed_package() {
    // getInstallationSource() => null (the default for a fresh package).
    let package = create_package_mock();

    let manager = create_manager();

    let result = manager.get_downloader_for_package(package);
    assert!(result.is_err());
}

#[test]
fn test_get_downloader_for_metapackage() {
    let package = create_package_mock();
    package.__set_type(BString::from("metapackage"));

    let manager = create_manager();

    assert!(
        manager
            .get_downloader_for_package(package)
            .unwrap()
            .is_none()
    );
}

#[test]
fn test_get_downloader_for_correctly_installed_dist_package() {
    let package = create_package_mock();
    package.set_installation_source(Some(BString::from("dist")));
    package.set_dist_type(Some(BString::from("pear")));

    let downloader = as_dyn(downloader_mock(BStr::new("dist")));

    let mut manager = create_manager();
    manager.set_downloader("pear", downloader.clone());

    let result = manager
        .get_downloader_for_package(package)
        .unwrap()
        .unwrap();
    assert!(std::rc::Rc::ptr_eq(&downloader, &result));
}

#[test]
fn test_get_downloader_for_incorrectly_installed_dist_package() {
    let package = create_package_mock();
    package.set_installation_source(Some(BString::from("dist")));
    package.set_dist_type(Some(BString::from("git")));

    let downloader = as_dyn(downloader_mock(BStr::new("source")));

    let mut manager = create_manager();
    manager.set_downloader("git", downloader);

    // LogicException: the resolved downloader is a source downloader.
    assert!(manager.get_downloader_for_package(package).is_err());
}

#[test]
fn test_get_downloader_for_correctly_installed_source_package() {
    let package = create_package_mock();
    package.set_installation_source(Some(BString::from("source")));
    package.__set_source_type(Some(BString::from("git")));

    let downloader = as_dyn(downloader_mock(BStr::new("source")));

    let mut manager = create_manager();
    manager.set_downloader("git", downloader.clone());

    let result = manager
        .get_downloader_for_package(package)
        .unwrap()
        .unwrap();
    assert!(std::rc::Rc::ptr_eq(&downloader, &result));
}

#[test]
fn test_get_downloader_for_incorrectly_installed_source_package() {
    let package = create_package_mock();
    package.set_installation_source(Some(BString::from("source")));
    package.__set_source_type(Some(BString::from("pear")));

    let downloader = as_dyn(downloader_mock(BStr::new("dist")));

    let mut manager = create_manager();
    manager.set_downloader("pear", downloader);

    assert!(manager.get_downloader_for_package(package).is_err());
}

#[test]
fn test_full_package_download() {
    let package = create_package_mock();
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("dist"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(downloader));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("dist"))
    );
}

#[test]
fn test_full_package_download_failover() {
    let package = create_package_mock();
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    // dist downloader fails, source downloader (tried next) succeeds.
    let mut downloader_fail = downloader_mock(BStr::new("dist"));
    downloader_fail
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Err(RuntimeException::new(BString::from("Foo")).into()));

    let mut downloader_success = downloader_mock(BStr::new("source"));
    downloader_success
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(downloader_fail));
    manager.set_downloader("git", as_dyn(downloader_success));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    // PHP asserts setInstallationSource was called with 'dist' then 'source'
    // (withConsecutive/exactly(2)). The real package retains only the final
    // installation_source, so the dist-before-failover step is instead evidenced by
    // both downloaders' expect_download().times(1): the 'dist' downloader is reached
    // and fails, then the 'source' downloader is reached and succeeds. A faithful
    // ordering check would need a src-side set-history hook on Package.
    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_bad_package_download() {
    let package = create_package_mock();
    // getSourceType() => null, getDistType() => null.

    let manager = create_manager();

    assert!(run(manager.download(package, std::path::Path::new("target_dir"), None)).is_err());
}

#[test]
fn test_dist_only_package_download() {
    let package = create_package_mock();
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("dist"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(downloader));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("dist"))
    );
}

#[test]
fn test_source_only_package_download() {
    let package = create_package_mock();
    package.__set_source_type(Some(BString::from("git")));

    let mut downloader = downloader_mock(BStr::new("source"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("git", as_dyn(downloader));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_metapackage_package_download() {
    // There is no downloader for metapackages, so getDownloaderForPackage yields none.
    let package = create_package_mock();
    package.__set_source_type(Some(BString::from("git")));
    package.__set_type(BString::from("metapackage"));

    let manager = create_manager();

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_full_package_download_with_source_preferred() {
    let package = create_package_mock();
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("source"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("git", as_dyn(downloader));

    manager.set_prefer_source(true);
    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_dist_only_package_download_with_source_preferred() {
    let package = create_package_mock();
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("dist"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(downloader));

    manager.set_prefer_source(true);
    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("dist"))
    );
}

#[test]
fn test_source_only_package_download_with_source_preferred() {
    let package = create_package_mock();
    package.__set_source_type(Some(BString::from("git")));

    let mut downloader = downloader_mock(BStr::new("source"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("git", as_dyn(downloader));

    manager.set_prefer_source(true);
    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_bad_package_download_with_source_preferred() {
    let package = create_package_mock();
    // getSourceType() => null, getDistType() => null.

    let manager = create_manager();
    manager.set_prefer_source(true);

    assert!(run(manager.download(package, std::path::Path::new("target_dir"), None)).is_err());
}

#[test]
fn test_update_dist_with_equal_types() {
    let initial = create_package_mock();
    initial.set_installation_source(Some(BString::from("dist")));
    initial.set_dist_type(Some(BString::from("zip")));

    let target = create_package_mock();
    target.set_installation_source(Some(BString::from("dist")));
    target.set_dist_type(Some(BString::from("zip")));

    let mut zip_downloader = downloader_mock(BStr::new("dist"));
    zip_downloader
        .expect_update()
        .times(1)
        .withf(|_initial, _target, path| path == "vendor/bundles/FOS/UserBundle")
        .returning(|_, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("zip", as_dyn(zip_downloader));

    run(manager.update(
        initial,
        target,
        std::path::Path::new("vendor/bundles/FOS/UserBundle"),
    ))
    .unwrap();
}

#[test]
fn test_update_dist_with_not_equal_types() {
    let initial = create_package_mock();
    initial.set_installation_source(Some(BString::from("dist")));
    initial.set_dist_type(Some(BString::from("xz")));

    let target = create_package_mock();
    target.set_installation_source(Some(BString::from("dist")));
    target.set_dist_type(Some(BString::from("zip")));

    let mut xz_downloader = downloader_mock(BStr::new("dist"));
    xz_downloader
        .expect_remove_with_output()
        .times(1)
        .withf(|_pkg, path, _output| path == "vendor/bundles/FOS/UserBundle")
        .returning(|_, _, _| Ok(None));

    let mut zip_downloader = downloader_mock(BStr::new("dist"));
    zip_downloader
        .expect_install_with_output()
        .times(1)
        .withf(|_pkg, path, _output| path == "vendor/bundles/FOS/UserBundle")
        .returning(|_, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("xz", as_dyn(xz_downloader));
    manager.set_downloader("zip", as_dyn(zip_downloader));

    run(manager.update(
        initial,
        target,
        std::path::Path::new("vendor/bundles/FOS/UserBundle"),
    ))
    .unwrap();
}

#[test]
fn test_get_available_sources_update_sticks_to_same_source() {
    // ref: updatesProvider. Columns: prevPkgSource, prevPkgIsDev, targetAvailable,
    // targetIsDev, expected.
    struct Case {
        prev_pkg_source: Option<&'static BStr>,
        prev_pkg_is_dev: bool,
        target_available: &'static [&'static BStr],
        target_is_dev: bool,
        expected: &'static [&'static BStr],
    }

    const SOURCE: &BStr = php_str(b"source");
    const DIST: &BStr = php_str(b"dist");
    let cases = [
        // updates keep previous source as preference
        Case {
            prev_pkg_source: Some(SOURCE),
            prev_pkg_is_dev: false,
            target_available: &[SOURCE, DIST],
            target_is_dev: false,
            expected: &[SOURCE, DIST],
        },
        Case {
            prev_pkg_source: Some(DIST),
            prev_pkg_is_dev: false,
            target_available: &[SOURCE, DIST],
            target_is_dev: false,
            expected: &[DIST, SOURCE],
        },
        // updates do not keep previous source if target package does not have it
        Case {
            prev_pkg_source: Some(SOURCE),
            prev_pkg_is_dev: false,
            target_available: &[DIST],
            target_is_dev: false,
            expected: &[DIST],
        },
        Case {
            prev_pkg_source: Some(DIST),
            prev_pkg_is_dev: false,
            target_available: &[SOURCE],
            target_is_dev: false,
            expected: &[SOURCE],
        },
        // updates do not keep previous source if target is dev and prev wasn't dev and installed from dist
        Case {
            prev_pkg_source: Some(SOURCE),
            prev_pkg_is_dev: false,
            target_available: &[SOURCE, DIST],
            target_is_dev: true,
            expected: &[SOURCE, DIST],
        },
        Case {
            prev_pkg_source: Some(DIST),
            prev_pkg_is_dev: false,
            target_available: &[SOURCE, DIST],
            target_is_dev: true,
            expected: &[SOURCE, DIST],
        },
        // install picks the right default
        Case {
            prev_pkg_source: None,
            prev_pkg_is_dev: false,
            target_available: &[SOURCE, DIST],
            target_is_dev: true,
            expected: &[SOURCE, DIST],
        },
        Case {
            prev_pkg_source: None,
            prev_pkg_is_dev: false,
            target_available: &[DIST],
            target_is_dev: true,
            expected: &[DIST],
        },
        Case {
            prev_pkg_source: None,
            prev_pkg_is_dev: false,
            target_available: &[SOURCE],
            target_is_dev: true,
            expected: &[SOURCE],
        },
        Case {
            prev_pkg_source: None,
            prev_pkg_is_dev: false,
            target_available: &[SOURCE, DIST],
            target_is_dev: false,
            expected: &[DIST, SOURCE],
        },
        Case {
            prev_pkg_source: None,
            prev_pkg_is_dev: false,
            target_available: &[DIST],
            target_is_dev: false,
            expected: &[DIST],
        },
        Case {
            prev_pkg_source: None,
            prev_pkg_is_dev: false,
            target_available: &[SOURCE],
            target_is_dev: false,
            expected: &[SOURCE],
        },
    ];

    let manager = create_manager();

    for case in cases {
        let initial = case.prev_pkg_source.map(|source| {
            let package = make_package(BStr::new("dummy/pkg"), case.prev_pkg_is_dev);
            package.set_installation_source(Some(bformat!("{source}")));
            package
        });

        let target = make_package(BStr::new("dummy/pkg"), case.target_is_dev);
        if case.target_available.contains(&BStr::new("source")) {
            target.__set_source_type(Some(BString::from("git")));
        }
        if case.target_available.contains(&BStr::new("dist")) {
            target.set_dist_type(Some(BString::from("zip")));
        }

        let result = manager.__get_available_sources(target, initial).unwrap();
        let expected: Vec<BString> = case.expected.iter().map(|s| bformat!("{s}")).collect();
        assert_eq!(result, expected);
    }
}

#[test]
fn test_update_metapackage() {
    // There is no downloader for metapackages.
    let initial = create_package_mock();
    initial.__set_type(BString::from("metapackage"));
    let target = create_package_mock();
    target.__set_type(BString::from("metapackage"));

    let manager = create_manager();

    assert!(
        run(manager.update(initial, target, std::path::Path::new("vendor/pkg")))
            .unwrap()
            .is_none()
    );
}

#[test]
fn test_remove() {
    let package = create_package_mock();
    package.set_installation_source(Some(BString::from("dist")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut pear_downloader = downloader_mock(BStr::new("dist"));
    pear_downloader
        .expect_remove_with_output()
        .times(1)
        .withf(|_pkg, path, _output| path == "vendor/bundles/FOS/UserBundle")
        .returning(|_, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(pear_downloader));

    run(manager.remove(
        package,
        std::path::Path::new("vendor/bundles/FOS/UserBundle"),
    ))
    .unwrap();
}

#[test]
fn test_metapackage_remove() {
    // There is no downloader for metapackages.
    let package = create_package_mock();
    package.__set_type(BString::from("metapackage"));

    let manager = create_manager();

    assert!(
        run(manager.remove(
            package,
            std::path::Path::new("vendor/bundles/FOS/UserBundle")
        ))
        .unwrap()
        .is_none()
    );
}

#[test]
fn test_install_preference_without_preference_dev() {
    let package = make_package(BStr::new("dummy/pkg"), true);
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("source"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("git", as_dyn(downloader));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_install_preference_without_preference_no_dev() {
    let package = make_package(BStr::new("dummy/pkg"), false);
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("dist"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(downloader));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("dist"))
    );
}

#[test]
fn test_install_preference_without_match_dev() {
    let package = make_package(BStr::new("bar/package"), true);
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("source"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("git", as_dyn(downloader));
    manager.set_preferences(IndexMap::from([("foo/*".into(), "source".into())]));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_install_preference_without_match_no_dev() {
    let package = make_package(BStr::new("bar/package"), false);
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("dist"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(downloader));
    manager.set_preferences(IndexMap::from([("foo/*".into(), "source".into())]));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("dist"))
    );
}

#[test]
fn test_install_preference_with_match_auto_dev() {
    let package = make_package(BStr::new("foo/package"), true);
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("source"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("git", as_dyn(downloader));
    manager.set_preferences(IndexMap::from([("foo/*".into(), "auto".into())]));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_install_preference_with_match_auto_no_dev() {
    let package = make_package(BStr::new("foo/package"), false);
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("dist"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(downloader));
    manager.set_preferences(IndexMap::from([("foo/*".into(), "auto".into())]));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("dist"))
    );
}

#[test]
fn test_install_preference_with_match_source() {
    let package = make_package(BStr::new("foo/package"), false);
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("source"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("git", as_dyn(downloader));
    manager.set_preferences(IndexMap::from([("foo/*".into(), "source".into())]));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("source"))
    );
}

#[test]
fn test_install_preference_with_match_dist() {
    let package = make_package(BStr::new("foo/package"), false);
    package.__set_source_type(Some(BString::from("git")));
    package.set_dist_type(Some(BString::from("pear")));

    let mut downloader = downloader_mock(BStr::new("dist"));
    downloader
        .expect_download_with_output()
        .times(1)
        .withf(|_pkg, path, _prev, _output| path == "target_dir")
        .returning(|_, _, _, _| Ok(None));

    let mut manager = create_manager();
    manager.set_downloader("pear", as_dyn(downloader));
    manager.set_preferences(IndexMap::from([("foo/*".into(), "dist".into())]));

    run(manager.download(package.clone(), std::path::Path::new("target_dir"), None)).unwrap();

    assert_eq!(
        package.get_installation_source().as_ref(),
        Some(&bstr::BString::from("dist"))
    );
}
