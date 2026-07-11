//! ref: composer/tests/Composer/Test/Installer/MetapackageInstallerTest.php
//!
//! PHP verifies the mocked repository's add/remove/hasPackage calls; here the same
//! behaviour is checked against a real InstalledArrayRepository by observing its state.

use crate::test_case::get_package;
use shirabe::installer::{InstallerInterface, MetapackageInstaller};
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::{InstalledArrayRepository, RepositoryInterface};

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

fn installer() -> MetapackageInstaller {
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    MetapackageInstaller::new(io)
}

#[test]
fn test_install() {
    let package = get_package("test/pkg", "1.0.0");
    let mut installer = installer();
    let mut repository = InstalledArrayRepository::new_with_packages(vec![]).unwrap();

    run(installer.install(&mut repository, package.clone())).unwrap();

    assert!(repository.has_package(package));
}

#[test]
fn test_update() {
    let initial = get_package("test/initial", "1.0.0");
    let target = get_package("test/target", "1.0.1");
    let mut installer = installer();
    let mut repository =
        InstalledArrayRepository::new_with_packages(vec![initial.clone()]).unwrap();

    run(installer.update(&mut repository, initial.clone(), target.clone())).unwrap();

    assert!(!repository.has_package(initial.clone()));
    assert!(repository.has_package(target.clone()));

    // Updating again, with the initial package no longer installed, fails.
    assert!(run(installer.update(&mut repository, initial, target)).is_err());
}

#[test]
fn test_uninstall() {
    let package = get_package("test/pkg", "1.0.0");
    let mut installer = installer();
    let mut repository =
        InstalledArrayRepository::new_with_packages(vec![package.clone()]).unwrap();

    run(installer.uninstall(&mut repository, package.clone())).unwrap();

    assert!(!repository.has_package(package.clone()));

    // Uninstalling again, with the package no longer installed, fails.
    assert!(run(installer.uninstall(&mut repository, package)).is_err());
}
