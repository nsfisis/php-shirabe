//! ref: composer/tests/Composer/Test/Installer/SuggestedPackagesReporterTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::installer::SuggestedPackagesReporter;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;

/// Builds an IO mock and a SuggestedPackagesReporter over it. The IO mock
/// (`getIOMock`) is not available here, so this remains a stub.
fn set_up() {
    todo!()
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

// These construct a SuggestedPackagesReporter with a mocked IO and assert its accumulated
// suggestions and formatted output; mocking is not available here.
#[ignore = "asserts IO mock output via getIOMock()->expects(); no IO mocking infrastructure exists and BufferIO::new/get_output are todo!()"]
#[test]
fn test_constructor() {
    todo!()
}

#[ignore]
#[test]
fn test_get_packages_empty_by_default() {
    let reporter = reporter();
    assert!(reporter.get_packages().is_empty());
}

#[ignore]
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

#[ignore]
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

#[ignore = "addSuggestionsFromPackage test mocks Package::getSuggests; set_suggests only exists on RootPackageHandle, so the non-root Package fixture with suggests cannot be expressed"]
#[test]
fn test_add_suggestions_from_package() {
    todo!()
}

#[ignore = "asserts IO mock output via getIOMock()->expects(); no IO mocking infrastructure exists and BufferIO::new/get_output are todo!()"]
#[test]
fn test_output() {
    todo!()
}

#[ignore = "asserts IO mock output via getIOMock()->expects(); no IO mocking infrastructure exists and BufferIO::new/get_output are todo!()"]
#[test]
fn test_output_with_no_suggestion_reason() {
    todo!()
}

#[ignore = "asserts IO mock output via getIOMock()->expects(); no IO mocking infrastructure exists and BufferIO::new/get_output are todo!()"]
#[test]
fn test_output_ignores_formatting() {
    todo!()
}

#[ignore = "asserts IO mock output via getIOMock()->expects(); no IO mocking infrastructure exists and BufferIO::new/get_output are todo!()"]
#[test]
fn test_output_multiple_packages() {
    todo!()
}

#[ignore = "asserts IO mock output via getIOMock()->expects() and uses getMockBuilder mocks of InstalledRepository/PackageInterface; no mocking infrastructure exists"]
#[test]
fn test_output_skip_installed_packages() {
    todo!()
}

#[ignore = "uses getMockBuilder mock of InstalledRepository with ->expects($this->exactly(0)) call-count assertion; no mocking infrastructure exists"]
#[test]
fn test_output_not_getting_installed_packages_when_no_suggestions() {
    todo!()
}
