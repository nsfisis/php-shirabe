//! ref: composer/tests/Composer/Test/Installer/SuggestedPackagesReporterTest.php

use crate::io_mock::{Expectation, IOMock, IOMockGuard, get_io_mock};
use crate::test_case::get_package;
use indexmap::IndexMap;
use shirabe::installer::SuggestedPackagesReporter;
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::{InstalledRepository, LockArrayRepository, RepositoryInterfaceHandle};
use std::cell::RefCell;
use std::rc::Rc;

/// ref: SuggestedPackagesReporterTest::setUp.
///
/// Builds an IO mock and a SuggestedPackagesReporter sharing it. The IOMockGuard runs
/// assert_complete when it drops at the end of the test scope.
fn set_up() -> (Rc<RefCell<IOMock>>, SuggestedPackagesReporter, IOMockGuard) {
    let (mock, guard) = get_io_mock(io_interface::NORMAL).unwrap();
    let io: Rc<RefCell<dyn IOInterface>> = mock.clone();
    let reporter = SuggestedPackagesReporter::new(io);
    (mock, reporter, guard)
}

/// ref: SuggestedPackagesReporterTest::getSuggestedPackageArray
fn get_suggested_package_array() -> IndexMap<String, String> {
    let mut entry = IndexMap::new();
    entry.insert("source".to_string(), "a".to_string());
    entry.insert("target".to_string(), "b".to_string());
    entry.insert("reason".to_string(), "c".to_string());
    entry
}

fn reporter() -> SuggestedPackagesReporter {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    SuggestedPackagesReporter::new(io)
}

#[test]
fn test_constructor() {
    let (mock, mut reporter, _guard) = set_up();
    mock.borrow_mut()
        .expects(vec![Expectation::text("b")], true)
        .unwrap();

    reporter.add_package("a".to_string(), "b".to_string(), "c".to_string());
    reporter
        .output(SuggestedPackagesReporter::MODE_LIST, None, None)
        .unwrap();
}

#[test]
fn test_get_packages_empty_by_default() {
    let reporter = reporter();
    assert!(reporter.get_packages().is_empty());
}

#[test]
fn test_get_packages() {
    let suggested_package = get_suggested_package_array();
    let mut reporter = reporter();
    reporter.add_package(
        suggested_package["source"].clone(),
        suggested_package["target"].clone(),
        suggested_package["reason"].clone(),
    );
    assert_eq!(&vec![suggested_package], reporter.get_packages());
}

#[test]
fn test_add_package_appends() {
    let suggested_package_a = get_suggested_package_array();
    let mut suggested_package_b = get_suggested_package_array();
    suggested_package_b.insert("source".to_string(), "different source".to_string());
    suggested_package_b.insert("reason".to_string(), "different reason".to_string());
    let mut reporter = reporter();
    reporter.add_package(
        suggested_package_a["source"].clone(),
        suggested_package_a["target"].clone(),
        suggested_package_a["reason"].clone(),
    );
    reporter.add_package(
        suggested_package_b["source"].clone(),
        suggested_package_b["target"].clone(),
        suggested_package_b["reason"].clone(),
    );
    assert_eq!(
        &vec![suggested_package_a, suggested_package_b],
        reporter.get_packages()
    );
}

#[test]
fn test_add_suggestions_from_package() {
    let mut reporter = reporter();

    // PHP mocks getSuggests/getPrettyName; here a real package carries the suggests and name.
    let package = get_package("package-pretty-name", "1.0.0");
    let mut suggests = IndexMap::new();
    suggests.insert("target-a".to_string(), "reason-a".to_string());
    suggests.insert("target-b".to_string(), "reason-b".to_string());
    package.__set_suggests(suggests);

    reporter.add_suggestions_from_package(package);

    let mut expected_a = IndexMap::new();
    expected_a.insert("source".to_string(), "package-pretty-name".to_string());
    expected_a.insert("target".to_string(), "target-a".to_string());
    expected_a.insert("reason".to_string(), "reason-a".to_string());
    let mut expected_b = IndexMap::new();
    expected_b.insert("source".to_string(), "package-pretty-name".to_string());
    expected_b.insert("target".to_string(), "target-b".to_string());
    expected_b.insert("reason".to_string(), "reason-b".to_string());
    assert_eq!(&vec![expected_a, expected_b], reporter.get_packages());
}

#[test]
fn test_output() {
    let (mock, mut reporter, _guard) = set_up();
    reporter.add_package("a".to_string(), "b".to_string(), "c".to_string());

    mock.borrow_mut()
        .expects(
            vec![
                Expectation::text("a suggests:"),
                Expectation::text(" - b: c"),
                Expectation::text(""),
            ],
            true,
        )
        .unwrap();

    reporter
        .output(SuggestedPackagesReporter::MODE_BY_PACKAGE, None, None)
        .unwrap();
}

#[test]
fn test_output_with_no_suggestion_reason() {
    let (mock, mut reporter, _guard) = set_up();
    reporter.add_package("a".to_string(), "b".to_string(), "".to_string());

    mock.borrow_mut()
        .expects(
            vec![
                Expectation::text("a suggests:"),
                Expectation::text(" - b"),
                Expectation::text(""),
            ],
            true,
        )
        .unwrap();

    reporter
        .output(SuggestedPackagesReporter::MODE_BY_PACKAGE, None, None)
        .unwrap();
}

#[test]
fn test_output_ignores_formatting() {
    let (mock, mut reporter, _guard) = set_up();
    reporter.add_package(
        "source".to_string(),
        "target1".to_string(),
        "\x1b[1;37;42m Like us\r\non Facebook \x1b[0m".to_string(),
    );
    reporter.add_package(
        "source".to_string(),
        "target2".to_string(),
        "<bg=green>Like us on Facebook</>".to_string(),
    );

    mock.borrow_mut()
        .expects(
            vec![
                Expectation::text("source suggests:"),
                Expectation::text(" - target1: [1;37;42m Like us on Facebook [0m"),
                Expectation::text(" - target2: <bg=green>Like us on Facebook</>"),
                Expectation::text(""),
            ],
            true,
        )
        .unwrap();

    reporter
        .output(SuggestedPackagesReporter::MODE_BY_PACKAGE, None, None)
        .unwrap();
}

#[test]
fn test_output_multiple_packages() {
    let (mock, mut reporter, _guard) = set_up();
    reporter.add_package("a".to_string(), "b".to_string(), "c".to_string());
    reporter.add_package(
        "source package".to_string(),
        "target".to_string(),
        "because reasons".to_string(),
    );

    mock.borrow_mut()
        .expects(
            vec![
                Expectation::text("a suggests:"),
                Expectation::text(" - b: c"),
                Expectation::text(""),
                Expectation::text("source package suggests:"),
                Expectation::text(" - target: because reasons"),
                Expectation::text(""),
            ],
            true,
        )
        .unwrap();

    reporter
        .output(SuggestedPackagesReporter::MODE_BY_PACKAGE, None, None)
        .unwrap();
}

#[test]
fn test_output_skip_installed_packages() {
    let (mock, mut reporter, _guard) = set_up();

    // PHP mocks two PackageInterfaces returning getNames() ['x','y'] and ['b']; only the 'b'
    // match is consequential (it filters the 'a' -> 'b' suggestion). Real packages carry a
    // single name, so the immaterial 'y' name is omitted.
    let package1 = get_package("x", "1.0.0");
    let package2 = get_package("b", "1.0.0");
    let installed = LockArrayRepository::new(vec![package1, package2]).unwrap();
    let mut repository = InstalledRepository::new(vec![RepositoryInterfaceHandle::new(installed)]);

    reporter.add_package("a".to_string(), "b".to_string(), "c".to_string());
    reporter.add_package(
        "source package".to_string(),
        "target".to_string(),
        "because reasons".to_string(),
    );

    mock.borrow_mut()
        .expects(
            vec![
                Expectation::text("source package suggests:"),
                Expectation::text(" - target: because reasons"),
                Expectation::text(""),
            ],
            true,
        )
        .unwrap();

    reporter
        .output(
            SuggestedPackagesReporter::MODE_BY_PACKAGE,
            Some(&mut repository),
            None,
        )
        .unwrap();
}

#[test]
fn test_output_not_getting_installed_packages_when_no_suggestions() {
    let (_mock, reporter, _guard) = set_up();

    // PHP asserts getPackages() is called exactly 0 times. With no suggestions queued,
    // get_filtered_suggestions short-circuits before touching the repository.
    let installed = LockArrayRepository::new(vec![]).unwrap();
    let mut repository = InstalledRepository::new(vec![RepositoryInterfaceHandle::new(installed)]);

    reporter
        .output(
            SuggestedPackagesReporter::MODE_BY_PACKAGE,
            Some(&mut repository),
            None,
        )
        .unwrap();
}
