//! ref: composer/tests/Composer/Test/Platform/HhvmDetectorTest.php

use shirabe::platform::hhvm_detector::HhvmDetector;
use shirabe::platform::hhvm_detector::HhvmDetectorInterface;
use shirabe::util::Platform;
use shirabe::util::ProcessExecutor;
use shirabe_external_packages::symfony::process::ExecutableFinder;
use shirabe_php_shim::{PhpMixed, constant, defined};
use shirabe_semver::VersionParser;

fn set_up() -> HhvmDetector {
    let hhvm_detector = HhvmDetector::new(None, None);
    hhvm_detector.reset();
    hhvm_detector
}

#[test]
fn test_hhvm_version_when_executing_in_hhvm() {
    let mut hhvm_detector = set_up();
    if !defined("HHVM_VERSION_ID") {
        // markTestSkipped('Not running with HHVM')
        return;
    }
    let version = hhvm_detector.get_version();
    assert_eq!(version_id_to_version(), version);
}

#[test]
fn test_hhvm_version_when_executing_in_php() {
    let mut hhvm_detector = set_up();
    if defined("HHVM_VERSION_ID") {
        // markTestSkipped('Running with HHVM')
        return;
    }
    if Platform::is_windows() {
        // markTestSkipped('Test does not run on Windows')
        return;
    }
    let finder = ExecutableFinder::new();
    let hhvm = finder.find("hhvm", None, &[]);
    let hhvm = match hhvm {
        Some(hhvm) => hhvm,
        None => {
            // markTestSkipped('HHVM is not installed')
            return;
        }
    };

    let detected_version = hhvm_detector.get_version();
    assert!(detected_version.is_some(), "Failed to detect HHVM version");
    let detected_version = detected_version.unwrap();

    let mut process = ProcessExecutor::new(None);
    let mut version = PhpMixed::Null;
    let cmd = format!(
        "{} --php -d hhvm.jit=0 -r \"echo HHVM_VERSION;\" 2>/dev/null",
        ProcessExecutor::escape(&hhvm)
    );
    let exit_code = process.execute(cmd.as_str(), &mut version, None).unwrap();
    assert_eq!(0, exit_code);

    let version = version
        .as_string()
        .map(|s| s.to_string())
        .unwrap_or_default();
    assert_eq!(
        VersionParser.normalize(&version, None).unwrap(),
        VersionParser.normalize(&detected_version, None).unwrap()
    );
}

fn version_id_to_version() -> Option<String> {
    if !defined("HHVM_VERSION_ID") {
        return None;
    }

    let hhvm_version_id = constant("HHVM_VERSION_ID").as_int().unwrap();
    Some(format!(
        "{}.{}.{}",
        hhvm_version_id / 10000,
        (hhvm_version_id / 100) % 100,
        hhvm_version_id % 100,
    ))
}
