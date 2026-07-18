//! ref: composer/tests/Composer/Test/InstallerTest.php

#[path = "common/config_stub.rs"]
mod config_stub;
#[path = "common/test_case.rs"]
mod test_case;

use config_stub::ConfigStubBuilder;
use test_case::{get_package, get_version_constraint};

use indexmap::IndexMap;

use shirabe::advisory::{AuditConfig, Auditor};
use shirabe::autoload::{AutoloadGeneratorInterface, ClassLoader};
use shirabe::config::Config;
use shirabe::console::application::ApplicationHandle;
use shirabe::dependency_resolver::{Transaction, UpdateAllowTransitiveDeps};
use shirabe::downloader::{DownloadManagerInterface, DownloaderInterface};
use shirabe::event_dispatcher::{Callable, EventDispatcherInterface, EventInterface};
use shirabe::factory::{DisablePlugins, Factory, LocalConfigInput};
use shirabe::filter::platform_requirement_filter::{
    PlatformRequirementFilterFactory, PlatformRequirementFilterInterface,
};
use shirabe::installer::{InstallationManager, Installer};
use shirabe::io::IOInterface;
use shirabe::io::buffer_io::BufferIO;
use shirabe::json::JsonFile;
use shirabe::package::dumper::ArrayDumper;
use shirabe::package::{
    Link, Locker, LockerInterface, PackageInterfaceHandle, RootPackageHandle,
    RootPackageInterfaceHandle,
};
use shirabe::repository::{
    ArrayRepository, InstalledArrayRepository, InstalledRepositoryInterface,
    RepositoryInterfaceHandle, RepositoryManager, RepositoryManagerInterface,
};
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe::util::platform::Platform;
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_class_map_generator::class_map::ClassMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::console::command::command::Command as SymfonyCommand;
use shirabe_external_packages::symfony::console::command::command::CommandData;
use shirabe_external_packages::symfony::console::input::input_argument::InputArgument;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::input::input_option::InputOption;
use shirabe_external_packages::symfony::console::input::string_input::StringInput;
use shirabe_external_packages::symfony::console::output::output_interface::{
    OutputInterface, VERBOSITY_NORMAL,
};
use shirabe_external_packages::symfony::console::output::stream_output::StreamOutput;
use shirabe_php_shim::{PREG_SPLIT_DELIM_CAPTURE, PhpMixed, php_regex};
use shirabe_semver::VersionParser;
use shirabe_semver::constraint::AnyConstraint;

// The chdir back to prevCwd (cwd management) and removeDirectory of tempComposerHome (a
// path produced by the unported install pipeline) are not ported; only the env clears are.
fn tear_down() {
    Platform::clear_env("COMPOSER_POOL_OPTIMIZER");
    Platform::clear_env("COMPOSER_FUND");
}

struct TearDown;

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down();
    }
}

// PHP mocks `Composer\Downloader\DownloadManager` with getMockBuilder; PHPUnit mocks are permissive
// (every method returns null), so the Rust equivalent is a no-op stub over the trait seam.
#[derive(Debug)]
struct StubDownloadManager;

#[async_trait::async_trait(?Send)]
impl DownloadManagerInterface for StubDownloadManager {
    fn set_prefer_source(&mut self, _prefer_source: bool) {}
    fn set_prefer_dist(&mut self, _prefer_dist: bool) {}
    fn get_downloader_for_package(
        &self,
        _package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>>> {
        Ok(None)
    }
    async fn download(
        &self,
        _package: PackageInterfaceHandle,
        _target_dir: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }
    async fn prepare(
        &self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _target_dir: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }
    async fn install(
        &self,
        _package: PackageInterfaceHandle,
        _target_dir: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }
    async fn update(
        &self,
        _initial: PackageInterfaceHandle,
        _target: PackageInterfaceHandle,
        _target_dir: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }
    async fn remove(
        &self,
        _package: PackageInterfaceHandle,
        _target_dir: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }
    async fn cleanup(
        &self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _target_dir: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }
}

// PHP mocks `Composer\EventDispatcher\EventDispatcher` with disableOriginalConstructor()->getMock();
// a permissive no-op stub mirrors the PHPUnit mock.
#[derive(Debug)]
struct StubEventDispatcher;

impl EventDispatcherInterface for StubEventDispatcher {
    fn dispatch(
        &mut self,
        _event_name: Option<&str>,
        _event: Option<&mut dyn EventInterface>,
    ) -> anyhow::Result<i64> {
        Ok(0)
    }
    fn dispatch_script(
        &mut self,
        _event_name: &str,
        _dev_mode: bool,
        _additional_args: Vec<String>,
        _flags: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<i64> {
        Ok(0)
    }
    fn dispatch_installer_event(
        &mut self,
        _event_name: &str,
        _dev_mode: bool,
        _execute_operations: bool,
        _transaction: Transaction,
    ) -> anyhow::Result<i64> {
        Ok(0)
    }
    fn add_listener(&mut self, _event_name: &str, _listener: Callable, _priority: i64) {}
    fn has_event_listeners(&mut self, _event: &dyn EventInterface) -> bool {
        false
    }
}

// PHP mocks `Composer\Autoload\AutoloadGenerator` with disableOriginalConstructor()->getMock();
// a permissive no-op stub mirrors the PHPUnit mock.
#[derive(Debug)]
struct StubAutoloadGenerator;

impl AutoloadGeneratorInterface for StubAutoloadGenerator {
    fn set_dev_mode(&mut self, _dev_mode: bool) {}
    fn set_class_map_authoritative(&mut self, _class_map_authoritative: bool) {}
    fn set_apcu(&mut self, _apcu: bool, _apcu_prefix: Option<String>) {}
    fn set_run_scripts(&mut self, _run_scripts: bool) {}
    fn set_dry_run(&mut self, _dry_run: bool) {}
    fn set_platform_requirement_filter(
        &mut self,
        _platform_requirement_filter: std::rc::Rc<dyn PlatformRequirementFilterInterface>,
    ) {
    }
    #[allow(clippy::too_many_arguments)]
    fn dump(
        &mut self,
        _config: &Config,
        _local_repo: &mut dyn InstalledRepositoryInterface,
        _root_package: RootPackageInterfaceHandle,
        _installation_manager: &mut dyn shirabe::installer::InstallationManagerInterface,
        _target_dir: &str,
        _scan_psr_packages: bool,
        _suffix: Option<String>,
        _locker: Option<&mut dyn LockerInterface>,
        _strict_ambiguous: bool,
    ) -> anyhow::Result<ClassMap> {
        Ok(ClassMap::new())
    }
    fn build_package_map(
        &self,
        _installation_manager: &mut dyn shirabe::installer::InstallationManagerInterface,
        _root_package: RootPackageInterfaceHandle,
        _packages: Vec<PackageInterfaceHandle>,
    ) -> anyhow::Result<Vec<(PackageInterfaceHandle, Option<String>)>> {
        Ok(vec![])
    }
    fn parse_autoloads(
        &self,
        _package_map: Vec<(PackageInterfaceHandle, Option<String>)>,
        _root_package: RootPackageInterfaceHandle,
        _filtered_dev_packages: PhpMixed,
    ) -> IndexMap<String, PhpMixed> {
        IndexMap::new()
    }
    fn create_loader(
        &self,
        _autoloads: &IndexMap<String, PhpMixed>,
        _vendor_dir: Option<String>,
    ) -> ClassLoader {
        unimplemented!("create_loader is not reached by the installer test path")
    }
}

/// ref: TestCase::getPackage with class `Composer\Package\RootPackage`.
fn root_package(name: &str, version: &str) -> RootPackageHandle {
    let normalized = VersionParser.normalize(version, None).unwrap();
    RootPackageHandle::new(name.to_string(), normalized, version.to_string())
}

/// ref: `new Link($source, $target, $constraint, $type, $constraint->getPrettyString())`.
fn link(source: &str, target: &str, constraint: AnyConstraint, r#type: &str) -> Link {
    let pretty = constraint.get_pretty_string();
    Link::new(
        source.to_string(),
        target.to_string(),
        constraint,
        Some(r#type.to_string()),
        pretty,
    )
}

/// One row of `provideInstaller`.
struct InstallerCase {
    root_package: RootPackageHandle,
    repositories: Vec<RepositoryInterfaceHandle>,
    expected_install: Vec<PackageInterfaceHandle>,
    expected_update: Vec<(PackageInterfaceHandle, PackageInterfaceHandle)>,
    expected_uninstall: Vec<PackageInterfaceHandle>,
}

/// ref: InstallerTest::provideInstaller
fn provide_installer() -> Vec<InstallerCase> {
    let mut cases = vec![];

    // when A requires B and B requires A, and A is a non-published root package
    // the install of B should succeed
    let a = root_package("A", "1.0.0");
    a.set_requires(IndexMap::from([(
        "b".to_string(),
        link(
            "A",
            "B",
            get_version_constraint("=", "1.0.0"),
            Link::TYPE_REQUIRE,
        ),
    )]));
    let b = get_package("B", "1.0.0");
    b.as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "B",
                "A",
                get_version_constraint("=", "1.0.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    cases.push(InstallerCase {
        root_package: a,
        repositories: vec![RepositoryInterfaceHandle::new(
            ArrayRepository::new(vec![b.clone()]).unwrap(),
        )],
        expected_install: vec![b],
        expected_update: vec![],
        expected_uninstall: vec![],
    });

    // #480: when A requires B and B requires A, and A is a published root package
    // only B should be installed, as A is the root
    let a = root_package("A", "1.0.0");
    a.set_requires(IndexMap::from([(
        "b".to_string(),
        link(
            "A",
            "B",
            get_version_constraint("=", "1.0.0"),
            Link::TYPE_REQUIRE,
        ),
    )]));
    let b = get_package("B", "1.0.0");
    b.as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "a".to_string(),
            link(
                "B",
                "A",
                get_version_constraint("=", "1.0.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));

    cases.push(InstallerCase {
        root_package: a.clone(),
        repositories: vec![RepositoryInterfaceHandle::new(
            ArrayRepository::new(vec![a.into(), b.clone()]).unwrap(),
        )],
        expected_install: vec![b],
        expected_update: vec![],
        expected_uninstall: vec![],
    });

    // TODO why are there not more cases with uninstall/update?
    cases
}

/// ref: InstallerTest::makePackagesComparable
fn make_packages_comparable(
    packages: &[PackageInterfaceHandle],
) -> Vec<IndexMap<String, PhpMixed>> {
    let dumper = ArrayDumper::new();
    packages.iter().map(|p| dumper.dump(p.clone())).collect()
}

#[test]
#[ignore]
fn test_installer() {
    let _tear_down = TearDown;

    for case in provide_installer() {
        let io_buffer = std::rc::Rc::new(std::cell::RefCell::new(
            BufferIO::new(String::new(), VERBOSITY_NORMAL, None).unwrap(),
        ));
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_buffer.clone();

        let config = ConfigStubBuilder::new()
            .with("vendor-dir", PhpMixed::String("foo".to_string()))
            .with("lock", PhpMixed::Bool(true))
            .with("notify-on-install", PhpMixed::Bool(true))
            .build_shared();

        let download_manager: std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(StubDownloadManager));

        let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
            HttpDownloader::__new_mock(io.clone(), config.clone()),
        ));

        let mut repository_manager = RepositoryManager::new(
            io.clone(),
            config.clone(),
            http_downloader.clone(),
            None,
            None,
        );
        repository_manager.set_local_repository(RepositoryInterfaceHandle::new(
            InstalledArrayRepository::new().unwrap(),
        ));
        for repository in &case.repositories {
            repository_manager.add_repository(repository.clone());
        }
        let repository_manager: std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(repository_manager));

        let r#loop = std::rc::Rc::new(std::cell::RefCell::new(Loop::new(
            http_downloader.clone(),
            None,
        )));
        let installation_manager: std::rc::Rc<std::cell::RefCell<InstallationManager>> =
            std::rc::Rc::new(std::cell::RefCell::new(InstallationManager::__new_mock(
                r#loop,
                io.clone(),
                None,
            )));

        // emulate a writable lock file: a real JsonFile over a fresh temp path (initially absent, so
        // the installer falls back to an update; PHP uses an in-memory JsonFile mock instead).
        let lock_dir = tempfile::TempDir::new().unwrap();
        let lock_path = lock_dir.path().join("composer.lock");
        let lock_json =
            JsonFile::new(lock_path.to_string_lossy().into_owned(), None, None).unwrap();
        let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
            io.clone(),
        ))));
        let locker: std::rc::Rc<std::cell::RefCell<dyn LockerInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(Locker::new(
                io.clone(),
                lock_json,
                installation_manager.clone(),
                "{}",
                process,
            )));

        let autoload_generator: std::rc::Rc<std::cell::RefCell<dyn AutoloadGeneratorInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(StubAutoloadGenerator));

        let root_package: RootPackageInterfaceHandle =
            RootPackageInterfaceHandle::dup(&case.root_package.clone().into());
        let mut installer = Installer::new(
            io.clone(),
            config.clone(),
            root_package,
            download_manager,
            repository_manager,
            locker,
            installation_manager.clone(),
            std::rc::Rc::new(std::cell::RefCell::new(StubEventDispatcher)),
            autoload_generator,
        );
        installer.set_audit_config(
            AuditConfig::from_config(&mut config.borrow_mut(), false, Auditor::FORMAT_SUMMARY)
                .unwrap(),
        );
        let result = installer.run().unwrap();

        let output = io_buffer.borrow().get_output().replace('\r', "");
        assert_eq!(0, result, "{}", output);

        let installed = installation_manager.borrow().__get_installed_packages();
        assert_eq!(
            make_packages_comparable(&case.expected_install),
            make_packages_comparable(&installed),
            "{}",
            output
        );

        let updated = installation_manager.borrow().__get_updated_packages();
        assert_eq!(case.expected_update, updated);

        let uninstalled = installation_manager.borrow().__get_uninstalled_packages();
        assert_eq!(case.expected_uninstall, uninstalled);
    }
}

/// ref: PHPUnit assertStringMatchesFormat's StringMatchesFormatDescription::createPatternFromFormat.
fn create_pattern_from_format(format: &str) -> String {
    let escaped = regex::escape(format);
    let bytes = escaped.as_bytes();
    let mut out = String::from("(?s)^");
    let mut i = 0;
    while i < bytes.len() {
        // regex::escape turns "%" into "%" (it is not special) so the format codes survive intact.
        if bytes[i] == b'%' && i + 1 < bytes.len() {
            let replacement: Option<&str> = match bytes[i + 1] {
                b'%' => Some("%"),
                b'e' => Some("\\/"),
                b's' => Some("[^\\r\\n]+"),
                b'S' => Some("[^\\r\\n]*"),
                b'a' => Some(".+"),
                b'A' => Some(".*"),
                b'w' => Some("\\s*"),
                b'i' => Some("[+-]?\\d+"),
                b'd' => Some("\\d+"),
                b'x' => Some("[0-9a-fA-F]+"),
                b'f' => Some("[+-]?\\.?\\d+\\.?\\d*(?:[Ee][+-]?\\d+)?"),
                b'c' => Some("."),
                _ => None,
            };
            if let Some(replacement) = replacement {
                out.push_str(replacement);
                i += 2;
                continue;
            }
        }
        out.push(escaped[i..].chars().next().unwrap());
        i += escaped[i..].chars().next().unwrap().len_utf8();
    }
    out.push('$');
    out
}

/// ref: PHPUnit self::assertStringMatchesFormat.
fn assert_string_matches_format(format: &str, subject: &str, context: &str) {
    let pattern = create_pattern_from_format(format);
    let re = regex::Regex::new(&pattern)
        .unwrap_or_else(|e| panic!("invalid format pattern {}: {}", pattern, e));
    assert!(
        re.is_match(subject),
        "output does not match format.\n--- format ---\n{}\n--- output ---\n{}\n--- context ---\n{}",
        format,
        subject,
        context
    );
}

#[derive(Debug, Clone)]
enum ExpectLock {
    /// No EXPECT-LOCK section (`[]` in PHP); the lock is not asserted.
    Unset,
    /// EXPECT-LOCK is the literal string "false"; the lock must never be written.
    Never,
    /// EXPECT-LOCK holds an expected lock JSON.
    Json(serde_json::Value),
}

#[derive(Debug, Clone)]
enum ExpectResult {
    ExitCode(i64),
    /// EXPECT-EXCEPTION: the class-string of an expected exception.
    Exception(String),
}

#[derive(Debug, Clone)]
struct IntegrationCase {
    file: String,
    message: String,
    condition: Option<String>,
    composer: serde_json::Value,
    lock: Option<serde_json::Value>,
    installed: Option<serde_json::Value>,
    run: String,
    expect_lock: ExpectLock,
    expect_installed: Option<serde_json::Value>,
    expect_output: Option<String>,
    expect_output_optimized: Option<String>,
    expect: String,
    expect_result: ExpectResult,
}

fn fixtures_dir(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Fixtures")
        .join(path)
        .canonicalize()
        .unwrap()
}

/// ref: InstallerTest::readTestFile
fn read_test_file(
    file: &std::path::Path,
    fixtures_dir: &std::path::Path,
) -> IndexMap<String, String> {
    let contents = std::fs::read_to_string(file).unwrap();
    let tokens = Preg::split4(
        php_regex!(r"#(?:^|\n*)--([A-Z-]+)--\n#"),
        &contents,
        -1,
        PREG_SPLIT_DELIM_CAPTURE,
    );

    let section_info: [(&str, bool); 13] = [
        ("TEST", true),
        ("CONDITION", false),
        ("COMPOSER", true),
        ("LOCK", false),
        ("INSTALLED", false),
        ("RUN", true),
        ("EXPECT-LOCK", false),
        ("EXPECT-INSTALLED", false),
        ("EXPECT-OUTPUT", false),
        ("EXPECT-OUTPUT-OPTIMIZED", false),
        ("EXPECT-EXIT-CODE", false),
        ("EXPECT-EXCEPTION", false),
        ("EXPECT", true),
    ];
    let known: indexmap::IndexSet<&str> = section_info.iter().map(|(k, _)| *k).collect();

    let mut section: Option<String> = None;
    let mut data: IndexMap<String, String> = IndexMap::new();
    for token in tokens {
        if section.is_none() && token.is_empty() {
            continue; // skip leading blank
        }
        if section.is_none() {
            assert!(
                known.contains(token.as_str()),
                "The test file \"{}\" must not contain a section named \"{}\".",
                file.display(),
                token
            );
            section = Some(token);
            continue;
        }
        let sec = section.take().unwrap();
        data.insert(sec, token);
    }

    for (sec, required) in section_info {
        if required {
            assert!(
                data.contains_key(sec),
                "The test file \"{}\" must have a section named \"{}\".",
                file.display(),
                sec
            );
        }
    }
    let _ = fixtures_dir;
    data
}

fn collect_test_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            collect_test_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("test") {
            out.push(path);
        }
    }
}

/// ref: InstallerTest::loadIntegrationTests
fn load_integration_tests(path: &str) -> Vec<IntegrationCase> {
    let dir = fixtures_dir(path);
    let mut files = Vec::new();
    collect_test_files(&dir, &mut files);
    files.sort();

    let mut tests = Vec::new();
    for file in files {
        let test_data = read_test_file(&file, &dir);

        // skip 64bit related tests on 32bit (usize is 64-bit here, so this never triggers).
        if test_data
            .get("EXPECT-OUTPUT")
            .map(|s| s.contains("php-64bit"))
            .unwrap_or(false)
            && (usize::BITS == 32)
        {
            continue;
        }

        let message = test_data["TEST"].clone();
        let condition = test_data
            .get("CONDITION")
            .filter(|s| !s.is_empty())
            .cloned();
        let mut composer: serde_json::Value = serde_json::from_str(&test_data["COMPOSER"]).unwrap();

        if let Some(repositories) = composer.get_mut("repositories") {
            let fixtures_str = dir.to_string_lossy().replace('\\', "/");
            let rewrite = |repo: &mut serde_json::Value| {
                if repo.get("type").and_then(|t| t.as_str()) != Some("composer") {
                    return;
                }
                if let Some(url) = repo.get("url").and_then(|u| u.as_str())
                    && Preg::is_match(php_regex!(r"{^file://[^/]}"), url)
                {
                    let new_url = format!("file://{}/{}", fixtures_str, &url[7..]);
                    repo["url"] = serde_json::Value::String(new_url);
                }
            };
            match repositories {
                serde_json::Value::Array(list) => list.iter_mut().for_each(rewrite),
                serde_json::Value::Object(map) => map.values_mut().for_each(rewrite),
                _ => {}
            }
        }

        let lock = test_data.get("LOCK").filter(|s| !s.is_empty()).map(|s| {
            let mut lock: serde_json::Value = serde_json::from_str(s).unwrap();
            if lock.get("hash").is_none() {
                let encoded = JsonFile::encode_with_options(
                    &composer,
                    shirabe::json::JsonEncodeOptions::none(),
                );
                let hash = format!("{:x}", md5::compute(encoded.as_bytes()));
                lock["hash"] = serde_json::Value::String(hash);
            }
            lock
        });

        let installed = test_data
            .get("INSTALLED")
            .filter(|s| !s.is_empty())
            .map(|s| serde_json::from_str(s).unwrap());

        let run = test_data["RUN"].clone();

        let expect_lock = match test_data.get("EXPECT-LOCK").filter(|s| !s.is_empty()) {
            None => ExpectLock::Unset,
            Some(s) if s == "false" => ExpectLock::Never,
            Some(s) => ExpectLock::Json(serde_json::from_str(s).unwrap()),
        };

        let expect_installed = test_data
            .get("EXPECT-INSTALLED")
            .filter(|s| !s.is_empty())
            .map(|s| serde_json::from_str(s).unwrap());

        let expect_output = test_data.get("EXPECT-OUTPUT").cloned();
        let expect_output_optimized = test_data.get("EXPECT-OUTPUT-OPTIMIZED").cloned();
        let expect = test_data["EXPECT"].clone();

        let expect_result =
            if let Some(exc) = test_data.get("EXPECT-EXCEPTION").filter(|s| !s.is_empty()) {
                assert!(
                    test_data
                        .get("EXPECT-EXIT-CODE")
                        .filter(|s| !s.is_empty())
                        .is_none(),
                    "EXPECT-EXCEPTION and EXPECT-EXIT-CODE are mutually exclusive"
                );
                ExpectResult::Exception(exc.clone())
            } else if let Some(code) = test_data.get("EXPECT-EXIT-CODE").filter(|s| !s.is_empty()) {
                ExpectResult::ExitCode(code.trim().parse().unwrap())
            } else {
                ExpectResult::ExitCode(0)
            };

        tests.push(IntegrationCase {
            file: file
                .strip_prefix(&dir)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            message,
            condition,
            composer,
            lock,
            installed,
            run,
            expect_lock,
            expect_installed,
            expect_output,
            expect_output_optimized,
            expect,
            expect_result,
        });
    }

    tests
}

/// ref: the inline `eval($condition)` in doTestIntegration, ported for the known fixture conditions.
fn evaluate_condition(condition: &str) -> bool {
    match condition.trim() {
        // putenv() returns true on success, so these conditions always run the test (with the env set).
        "putenv('COMPOSER_FUND=1')" => {
            Platform::put_env("COMPOSER_FUND", "1");
            true
        }
        "putenv('COMPOSER_FUND=0')" => {
            Platform::put_env("COMPOSER_FUND", "0");
            true
        }
        // HHVM is never defined under the Rust port.
        "!defined('HHVM_VERSION')" => true,
        // TODO(phase-d): unported CONDITION expression (PHP eval has no Rust equivalent).
        other => panic!("// TODO(phase-d): unported CONDITION: {}", other),
    }
}

fn opt_bool(input: &dyn InputInterface, name: &str) -> bool {
    input
        .get_option(name)
        .ok()
        .and_then(|m| m.as_bool())
        .unwrap_or(false)
}

/// ref: `$ignorePlatformReqs = true === getOption('ignore-platform-reqs') ?: (getOption('ignore-platform-req') ?: false)`.
fn ignore_platform_reqs_value(input: &dyn InputInterface) -> PhpMixed {
    if opt_bool(input, "ignore-platform-reqs") {
        return PhpMixed::Bool(true);
    }
    let list = input
        .get_option("ignore-platform-req")
        .unwrap_or(PhpMixed::Bool(false));
    match &list {
        PhpMixed::List(items) if !items.is_empty() => list,
        PhpMixed::Array(map) if !map.is_empty() => list,
        _ => PhpMixed::Bool(false),
    }
}

fn write_json(path: &std::path::Path, value: &serde_json::Value) {
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
}

/// ref: InstallerTest::doTestIntegration
fn do_test_integration(case: &IntegrationCase, expect_output: Option<&str>) {
    if let Some(condition) = &case.condition
        && !evaluate_condition(condition)
    {
        return; // markTestSkipped
    }

    let io_buffer = std::rc::Rc::new(std::cell::RefCell::new(
        BufferIO::new(String::new(), VERBOSITY_NORMAL, None).unwrap(),
    ));
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_buffer.clone();

    let is_exception = matches!(case.expect_result, ExpectResult::Exception(_));

    // Create Composer mock object according to configuration (FactoryMock::create).
    let composer_str = serde_json::to_string(&case.composer).unwrap();
    let composer_data = JsonFile::parse_json(Some(&composer_str), None)
        .unwrap()
        .as_array()
        .cloned()
        .unwrap_or_default();
    let composer = Factory::__create_mock(
        io.clone(),
        Some(LocalConfigInput::Data(composer_data)),
        DisablePlugins::None,
        false,
    )
    .unwrap();

    // installed.json mock: a real JsonFile over a temp file holding $installed, wrapped in the
    // no-op InstalledFilesystemRepositoryMock.
    let installed_dir = tempfile::TempDir::new().unwrap();
    let installed_path = installed_dir.path().join("installed.json");
    write_json(
        &installed_path,
        case.installed.as_ref().unwrap_or(&serde_json::json!([])),
    );
    let installed_json =
        JsonFile::new(installed_path.to_string_lossy().into_owned(), None, None).unwrap();
    let local_repo = shirabe::repository::InstalledFilesystemRepository::__new_mock(
        installed_json,
        false,
        None,
        None,
    )
    .unwrap();
    let repository_manager = composer.borrow().get_repository_manager();
    repository_manager
        .borrow_mut()
        .set_local_repository(RepositoryInterfaceHandle::new(local_repo));

    // emulate a writable lock file: a real composer.lock over a temp path.
    let lock_dir = tempfile::TempDir::new().unwrap();
    let lock_path = lock_dir.path().join("composer.lock");
    if let Some(lock) = &case.lock {
        write_json(&lock_path, lock);
    }
    let lock_before = std::fs::read_to_string(&lock_path).ok();
    let lock_json = JsonFile::new(lock_path.to_string_lossy().into_owned(), None, None).unwrap();

    // The Locker needs a concrete InstallationManager; build a fresh recording mock just for it. The
    // asserted trace comes from the composer's own installation manager (read via as_any below).
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io.clone(),
    ))));
    let locker_loop = composer.borrow().get_loop();
    let locker_im = std::rc::Rc::new(std::cell::RefCell::new(InstallationManager::__new_mock(
        locker_loop,
        io.clone(),
        None,
    )));
    let contents = serde_json::to_string(&case.composer).unwrap();
    let locker = Locker::new(io.clone(), lock_json, locker_im, &contents, process);
    composer
        .borrow_mut()
        .set_locker(std::rc::Rc::new(std::cell::RefCell::new(locker)));

    composer
        .borrow_mut()
        .set_autoload_generator(std::rc::Rc::new(std::cell::RefCell::new(
            StubAutoloadGenerator,
        )));
    composer
        .borrow_mut()
        .set_event_dispatcher(std::rc::Rc::new(std::cell::RefCell::new(
            StubEventDispatcher,
        )));

    let installer = std::rc::Rc::new(std::cell::RefCell::new(Installer::create(
        io.clone(),
        &composer.upcast(),
    )));

    // Application with inline install/update commands (setCode closures).
    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    application.set_catch_exceptions(false);

    let run_result: std::rc::Rc<std::cell::RefCell<Option<anyhow::Result<i64>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));

    let install = std::rc::Rc::new(std::cell::RefCell::new(CommandData::new(Some(
        "install".to_string(),
    ))));
    {
        let install_ref = install.borrow();
        install_ref
            .add_option(
                "ignore-platform-reqs",
                PhpMixed::Null,
                Some(InputOption::VALUE_NONE),
                "",
                PhpMixed::Null,
            )
            .unwrap();
        install_ref
            .add_option(
                "ignore-platform-req",
                PhpMixed::Null,
                Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY),
                "",
                PhpMixed::Null,
            )
            .unwrap();
        install_ref
            .add_option(
                "no-dev",
                PhpMixed::Null,
                Some(InputOption::VALUE_NONE),
                "",
                PhpMixed::Null,
            )
            .unwrap();
        install_ref
            .add_option(
                "dry-run",
                PhpMixed::Null,
                Some(InputOption::VALUE_NONE),
                "",
                PhpMixed::Null,
            )
            .unwrap();
        let installer_cl = installer.clone();
        let composer_cl = composer.clone();
        let run_result_cl = run_result.clone();
        install_ref.set_code(Box::new(move |input, _output| {
            let ignore = ignore_platform_reqs_value(input);
            let mut inst = installer_cl.borrow_mut();
            inst.set_dev_mode(!opt_bool(input, "no-dev"))
                .set_dry_run(opt_bool(input, "dry-run"))
                .set_platform_requirement_filter(
                    PlatformRequirementFilterFactory::from_bool_or_list(ignore).unwrap(),
                )
                .set_audit_config(
                    AuditConfig::from_config(
                        &mut composer_cl.borrow().get_config().borrow_mut(),
                        false,
                        Auditor::FORMAT_SUMMARY,
                    )
                    .unwrap(),
                );
            let r = inst.run();
            let code = match &r {
                Ok(c) => *c,
                Err(_) => 1,
            };
            *run_result_cl.borrow_mut() = Some(r);
            PhpMixed::Int(code)
        }));
    }
    application
        .add(install.clone() as std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>)
        .unwrap();

    let update = std::rc::Rc::new(std::cell::RefCell::new(CommandData::new(Some(
        "update".to_string(),
    ))));
    {
        let update_ref = update.borrow();
        for (name, mode) in [
            ("ignore-platform-reqs", InputOption::VALUE_NONE),
            ("no-dev", InputOption::VALUE_NONE),
            ("no-install", InputOption::VALUE_NONE),
            ("dry-run", InputOption::VALUE_NONE),
            ("lock", InputOption::VALUE_NONE),
            ("with-all-dependencies", InputOption::VALUE_NONE),
            ("with-dependencies", InputOption::VALUE_NONE),
            ("minimal-changes", InputOption::VALUE_NONE),
            ("prefer-stable", InputOption::VALUE_NONE),
            ("prefer-lowest", InputOption::VALUE_NONE),
        ] {
            update_ref
                .add_option(name, PhpMixed::Null, Some(mode), "", PhpMixed::Null)
                .unwrap();
        }
        update_ref
            .add_option(
                "ignore-platform-req",
                PhpMixed::Null,
                Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY),
                "",
                PhpMixed::Null,
            )
            .unwrap();
        update_ref
            .add_argument(
                "packages",
                Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
                "",
                PhpMixed::Null,
            )
            .unwrap();
        let installer_cl = installer.clone();
        let composer_cl = composer.clone();
        let run_result_cl = run_result.clone();
        update_ref.set_code(Box::new(move |input, _output| {
            let packages: Vec<String> =
                match input.get_argument("packages").unwrap_or(PhpMixed::Null) {
                    PhpMixed::List(items) => items
                        .into_iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect(),
                    _ => vec![],
                };
            let filtered: Vec<String> = packages
                .iter()
                .filter(|p| !["lock", "nothing", "mirrors"].contains(&p.as_str()))
                .cloned()
                .collect();
            let update_mirrors = opt_bool(input, "lock") || filtered.len() != packages.len();

            let update_allow_transitive = if opt_bool(input, "with-all-dependencies") {
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDeps
            } else if opt_bool(input, "with-dependencies") {
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDepsNoRootRequire
            } else {
                UpdateAllowTransitiveDeps::UpdateOnlyListed
            };

            let ignore = ignore_platform_reqs_value(input);

            let mut inst = installer_cl.borrow_mut();
            inst.set_dev_mode(!opt_bool(input, "no-dev"))
                .set_update(true)
                .set_install(!opt_bool(input, "no-install"))
                .set_dry_run(opt_bool(input, "dry-run"))
                .set_update_mirrors(update_mirrors)
                .set_update_allow_list(filtered)
                .set_update_allow_transitive_dependencies(update_allow_transitive)
                .unwrap()
                .set_prefer_stable(opt_bool(input, "prefer-stable"))
                .set_prefer_lowest(opt_bool(input, "prefer-lowest"))
                .set_platform_requirement_filter(
                    PlatformRequirementFilterFactory::from_bool_or_list(ignore).unwrap(),
                )
                .set_audit_config(
                    AuditConfig::from_config(
                        &mut composer_cl.borrow().get_config().borrow_mut(),
                        false,
                        Auditor::FORMAT_SUMMARY,
                    )
                    .unwrap(),
                )
                .set_minimal_update(opt_bool(input, "minimal-changes"));
            let r = inst.run();
            let code = match &r {
                Ok(c) => *c,
                Err(_) => 1,
            };
            *run_result_cl.borrow_mut() = Some(r);
            PhpMixed::Int(code)
        }));
    }
    application
        .add(update.clone() as std::rc::Rc<std::cell::RefCell<dyn SymfonyCommand>>)
        .unwrap();

    assert!(
        Preg::is_match(r"{^(install|update)\b}", &case.run),
        "The run command only supports install and update"
    );

    let app_output_stream = shirabe_php_shim::php_fopen_resource("php://memory", "w+");
    let app_output = StreamOutput::new(app_output_stream.clone(), None, None, None)
        .unwrap()
        .expect("php://memory is a valid stream");
    let mut string_input = StringInput::new(&format!("{} -vvv", case.run)).unwrap();
    string_input.set_interactive(false);
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(string_input));
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(app_output));

    let app_run = application.run(Some(input), Some(output));

    let output_string = io_buffer.borrow().get_output().replace('\r', "");

    // Shouldn't check output and results if an exception was expected by this point.
    if is_exception {
        let ExpectResult::Exception(_) = &case.expect_result else {
            unreachable!()
        };
        let normalized = case.expect.replace('\n', shirabe_php_shim::PHP_EOL);
        let normalized = normalized.trim_end();
        let err = match run_result.borrow().as_ref() {
            Some(Err(e)) => format!("{}", e),
            _ => app_run
                .as_ref()
                .err()
                .map(|e| format!("{}", e))
                .unwrap_or_default(),
        };
        assert!(
            err.contains(normalized),
            "expected exception message containing:\n{}\n--- got ---\n{}",
            normalized,
            err
        );
        return;
    }

    let result = match run_result.borrow().as_ref() {
        Some(Ok(c)) => *c,
        Some(Err(e)) => panic!("installer run failed: {}\n{}", e, output_string),
        None => app_run.unwrap_or(-1) as i64,
    };

    let ExpectResult::ExitCode(expect_result) = &case.expect_result else {
        unreachable!()
    };
    shirabe_php_shim::rewind(&app_output_stream);
    let app_output_contents =
        shirabe_php_shim::stream_get_contents(&app_output_stream).unwrap_or_default();
    assert_eq!(
        *expect_result, result,
        "{}{}",
        output_string, app_output_contents
    );

    if let ExpectLock::Json(expect_lock) = &case.expect_lock {
        let actual = std::fs::read_to_string(&lock_path).unwrap();
        let mut actual_lock: serde_json::Value = serde_json::from_str(&actual).unwrap();
        if let Some(obj) = actual_lock.as_object_mut() {
            for k in ["hash", "content-hash", "_readme", "plugin-api-version"] {
                obj.remove(k);
            }
        }
        let mut expect_lock = expect_lock.clone();
        // PHP turns the empty-array sentinel into stdClass; serde compares {} vs [] strictly, so
        // normalize the known object-valued keys to {} when they are empty.
        if let Some(obj) = expect_lock.as_object_mut() {
            for k in ["stability-flags", "platform", "platform-dev"] {
                if obj.get(k) == Some(&serde_json::json!([])) {
                    obj.insert(k.to_string(), serde_json::json!({}));
                }
            }
        }
        assert_eq!(expect_lock, actual_lock);
    } else if let ExpectLock::Never = &case.expect_lock {
        let lock_after = std::fs::read_to_string(&lock_path).ok();
        assert_eq!(lock_before, lock_after, "lock file must not be written");
    }

    if let Some(expect_installed) = &case.expect_installed {
        let dumper = ArrayDumper::new();
        let local_repo = repository_manager.borrow().get_local_repository();
        let mut actual_installed: Vec<IndexMap<String, PhpMixed>> = local_repo
            .get_canonical_packages()
            .unwrap()
            .into_iter()
            .map(|package| {
                let mut dumped = dumper.dump(package);
                dumped.shift_remove("version_normalized");
                dumped
            })
            .collect();
        actual_installed.sort_by(
            |a: &IndexMap<String, PhpMixed>, b: &IndexMap<String, PhpMixed>| {
                let an = a
                    .get("name")
                    .and_then(|m| m.as_string())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let bn = b
                    .get("name")
                    .and_then(|m| m.as_string())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                an.cmp(&bn)
            },
        );
        // Faithful comparison would dump expect_installed through the same shape; we compare the
        // serialized forms so the assertion still fails loudly on divergence.
        let actual_json = serde_json::to_value(
            actual_installed
                .iter()
                .map(php_mixed_map_to_json)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(expect_installed, &actual_json);
    }

    // trace from the composer's recording InstallationManager.
    let im_handle = composer.borrow().get_installation_manager();
    let im_ref = im_handle.borrow();
    let trace = im_ref
        .as_any()
        .downcast_ref::<InstallationManager>()
        .expect("composer installation manager is the recording mock")
        .__get_trace();
    assert_eq!(case.expect.trim_end(), trace.join("\n"));

    if let Some(expect_output) = expect_output
        && !expect_output.is_empty()
    {
        let output = Preg::replace(
            php_regex!(r"{^    - .*?\.ini$}m"),
            "__inilist__",
            &output_string,
        );
        let output = Preg::replace(
            php_regex!(r"{(__inilist__\r?\n)+}"),
            "__inilist__\n",
            &output,
        );
        assert_string_matches_format(expect_output.trim_end(), output.trim_end(), &output_string);
    }
}

fn php_mixed_map_to_json(map: &IndexMap<String, PhpMixed>) -> serde_json::Value {
    serde_json::to_value(map).unwrap_or(serde_json::Value::Null)
}

#[test]
#[ignore = "ported; exercises the full install pipeline which is not yet executable end-to-end (execute_batch / repository / autoload stubs), so cases are expected to fail at runtime"]
fn test_slow_integration() {
    let _tear_down = TearDown;
    for case in load_integration_tests("installer-slow/") {
        Platform::clear_env("COMPOSER_FUND");
        Platform::put_env("COMPOSER_POOL_OPTIMIZER", "0");
        let expect_output = case.expect_output.clone();
        do_test_integration(&case, expect_output.as_deref());
    }
}

#[test]
#[ignore = "ported; exercises the full install pipeline which is not yet executable end-to-end (execute_batch / repository / autoload stubs), so cases are expected to fail at runtime"]
fn test_integration_with_pool_optimizer() {
    let _tear_down = TearDown;
    for case in load_integration_tests("installer/") {
        Platform::clear_env("COMPOSER_FUND");
        Platform::put_env("COMPOSER_POOL_OPTIMIZER", "1");
        let expect_output = case
            .expect_output_optimized
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| case.expect_output.clone());
        do_test_integration(&case, expect_output.as_deref());
    }
}

#[test]
#[ignore = "ported; exercises the full install pipeline which is not yet executable end-to-end (execute_batch / repository / autoload stubs), so cases are expected to fail at runtime"]
fn test_integration_with_raw_pool() {
    let _tear_down = TearDown;
    for case in load_integration_tests("installer/") {
        Platform::clear_env("COMPOSER_FUND");
        Platform::put_env("COMPOSER_POOL_OPTIMIZER", "0");
        let expect_output = case.expect_output.clone();
        do_test_integration(&case, expect_output.as_deref());
    }
}
