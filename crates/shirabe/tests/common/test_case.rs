//! Shared test helpers ported from composer/tests/Composer/Test/TestCase.php.
//!
//! Included into each integration-test binary that needs them via
//! `#[path = "../common/test_case.rs"] mod test_case;`.
#![allow(dead_code)]

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::console::application::ApplicationHandle;
use shirabe::installer::InstallationManager;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::json::JsonFile;
use shirabe::package::Locker;
use shirabe::package::handle::{
    CompleteAliasPackageHandle, CompletePackageHandle, PackageInterfaceHandle,
};
use shirabe::repository::{InstalledFilesystemRepository, WritableRepositoryInterface};
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe::util::platform::Platform;
use shirabe::util::process_executor::ProcessExecutor;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::input::streamable_input_interface::StreamableInputInterface;
use shirabe_external_packages::symfony::console::output::console_output::ConsoleOutput;
use shirabe_external_packages::symfony::console::output::console_output_interface::ConsoleOutputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_external_packages::symfony::console::output::stream_output::StreamOutput;
use shirabe_php_shim::{PhpMixed, PhpResource};
use shirabe_semver::VersionParser;
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};
use std::path::PathBuf;
use tempfile::TempDir;

/// ref: TestCase::getPackage (default class CompletePackage)
pub fn get_package(name: &str, version: &str) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    CompletePackageHandle::new(name.to_string(), norm_version, version.to_string()).into()
}

/// ref: TestCase::getPackage but returning the concrete `CompletePackageHandle` so the
/// `CompletePackage`-only setters (`set_license`, `set_homepage`, ...) are reachable.
pub fn get_complete_package(name: &str, version: &str) -> CompletePackageHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    CompletePackageHandle::new(name.to_string(), norm_version, version.to_string())
}

/// ref: TestCase::getAliasPackage (default class CompleteAliasPackage)
pub fn get_alias_package(
    package: &PackageInterfaceHandle,
    version: &str,
) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    let complete = CompletePackageHandle::from_rc_unchecked(package.as_rc().clone());
    CompleteAliasPackageHandle::new(complete, norm_version, version.to_string()).into()
}

/// ref: TestCase::getVersionConstraint
pub fn get_version_constraint(operator: &str, version: &str) -> AnyConstraint {
    let normalized = VersionParser.normalize(version, None).unwrap();
    AnyConstraint::Simple(SimpleConstraint::new(
        operator.to_string(),
        normalized,
        Some(format!("{} {}", operator, version)),
    ))
}

/// ref: TestCase::initTempComposer plus the running TearDown.
///
/// Creates a fresh temp dir, chdir()s into it, points `COMPOSER_HOME` at it, and writes
/// `composer.json`/`auth.json` (and `composer.lock` when given). The returned guard restores the
/// previous cwd / `COMPOSER_HOME` and removes the temp tree on drop, mirroring PHPUnit's `tearDown`.
pub struct TearDown {
    temp_dir: TempDir,
    prev_cwd: PathBuf,
    prev_composer_home: Option<String>,
}

impl TearDown {
    /// The temp directory created by `init_temp_composer`. Equivalent to the `$dir` returned by PHP.
    pub fn working_dir(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        // Restore the cwd before the TempDir field is dropped so the tree (possibly the cwd itself)
        // can be removed cleanly, even on a panicking test.
        let _ = std::env::set_current_dir(&self.prev_cwd);
        match &self.prev_composer_home {
            Some(value) => Platform::put_env("COMPOSER_HOME", value),
            None => Platform::clear_env("COMPOSER_HOME"),
        }
    }
}

/// ref: TestCase::initTempComposer
pub fn init_temp_composer(
    composer_json: Option<&serde_json::Value>,
    auth_json: Option<&serde_json::Value>,
    composer_lock: Option<&serde_json::Value>,
    setup_repositories: bool,
) -> TearDown {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path().to_path_buf();

    let prev_cwd = std::env::current_dir().unwrap();
    let prev_composer_home = Platform::get_env("COMPOSER_HOME");

    Platform::put_env("COMPOSER_HOME", &format!("{}/composer-home", dir.display()));
    Platform::put_env("COMPOSER_DISABLE_XDEBUG_WARN", "1");

    let mut composer_json = composer_json
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let auth_json = auth_json.cloned().unwrap_or_else(|| serde_json::json!({}));

    if setup_repositories && let Some(repositories) = composer_json.get("repositories").cloned() {
        let packagist_false = serde_json::json!({ "packagist.org": false });
        let already_present = match &repositories {
            serde_json::Value::Object(map) => map.contains_key("packagist.org"),
            serde_json::Value::Array(list) => list.iter().any(|r| r == &packagist_false),
            _ => false,
        };
        if !already_present {
            match composer_json
                .get_mut("repositories")
                .and_then(|r| r.as_array_mut())
            {
                Some(list) => list.push(packagist_false),
                None => {
                    if let Some(map) = composer_json
                        .get_mut("repositories")
                        .and_then(|r| r.as_object_mut())
                    {
                        map.insert("packagist.org".to_string(), serde_json::Value::Bool(false));
                    }
                }
            }
        }
    }

    std::env::set_current_dir(&dir).unwrap();
    std::fs::write(
        dir.join("composer.json"),
        serde_json::to_string_pretty(&composer_json).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("auth.json"),
        serde_json::to_string_pretty(&auth_json).unwrap(),
    )
    .unwrap();
    if let Some(composer_lock) = composer_lock {
        std::fs::write(
            dir.join("composer.lock"),
            serde_json::to_string_pretty(composer_lock).unwrap(),
        )
        .unwrap();
    }

    TearDown {
        temp_dir,
        prev_cwd,
        prev_composer_home,
    }
}

fn null_io() -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
    std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()))
}

/// ref: FactoryMock::createInstallationManager (the real installers are never created in tests, so a
/// bare InstallationManager over a mock HttpDownloader suffices).
pub(crate) fn installation_manager(
    io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
) -> std::rc::Rc<std::cell::RefCell<InstallationManager>> {
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::__new_mock(
        io.clone(),
        config,
    )));
    let r#loop = std::rc::Rc::new(std::cell::RefCell::new(Loop::new(http_downloader, None)));
    std::rc::Rc::new(std::cell::RefCell::new(InstallationManager::new(
        r#loop,
        io.clone(),
        None,
    )))
}

/// ref: TestCase::createInstalledJson
///
/// Creates a `vendor/composer/installed.json` in the CWD (which `init_temp_composer` has chdir()'d
/// into) listing the given packages. `dev_packages` are recorded as the dev-package-name set.
pub fn create_installed_json(
    packages: &[PackageInterfaceHandle],
    dev_packages: &[PackageInterfaceHandle],
    dev_mode: bool,
) {
    std::fs::create_dir_all("vendor/composer").unwrap();
    let json_file =
        JsonFile::new("vendor/composer/installed.json".to_string(), None, None).unwrap();
    let mut repo = InstalledFilesystemRepository::new(json_file, false, None, None).unwrap();
    repo.set_dev_package_names(
        dev_packages
            .iter()
            .map(|pkg| pkg.get_pretty_name())
            .collect(),
    );
    for pkg in packages.iter().chain(dev_packages.iter()) {
        repo.add_package(pkg.clone()).unwrap();
        std::fs::create_dir_all(format!("vendor/{}", pkg.get_name())).unwrap();
    }

    let io = null_io();
    let im = installation_manager(&io);
    repo.write(dev_mode, &im.borrow()).unwrap();
}

/// ref: TestCase::createComposerLock
///
/// Creates a `composer.lock` in the CWD (chdir()'d into by `init_temp_composer`) listing the given
/// packages.
pub fn create_composer_lock(
    packages: &[PackageInterfaceHandle],
    dev_packages: &[PackageInterfaceHandle],
) {
    let io = null_io();
    let json_file = JsonFile::new("./composer.lock".to_string(), None, None).unwrap();
    let composer_file_contents = std::fs::read_to_string("./composer.json").unwrap_or_default();
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io.clone(),
    ))));
    let mut locker = Locker::new(
        io.clone(),
        json_file,
        installation_manager(&io),
        &composer_file_contents,
        process,
    );
    locker
        .set_lock_data(
            packages.to_vec(),
            Some(dev_packages.to_vec()),
            IndexMap::new(),
            IndexMap::new(),
            vec![],
            "dev",
            IndexMap::new(),
            false,
            false,
            IndexMap::new(),
            true,
        )
        .unwrap();
}

/// ref: TestCase::getApplicationTester
pub fn get_application_tester() -> ApplicationTester {
    let application = ApplicationHandle::new("Composer".to_string(), "".to_string()).unwrap();
    application.set_catch_exceptions(false);
    ApplicationTester::new(application)
}

/// ref: Symfony\Component\Console\Tester\ApplicationTester::run options.
///
/// Lives in the test harness (not `shirabe_external_packages`) because the tester drives shirabe's
/// own `ApplicationHandle`, which the external-packages crate cannot depend on.
#[derive(Debug, Default)]
pub struct RunOptions {
    pub interactive: Option<bool>,
    pub decorated: Option<bool>,
    pub verbosity: Option<i64>,
    pub capture_stderr_separately: bool,
}

/// ref: Symfony\Component\Console\Tester\ApplicationTester (with TesterTrait inlined).
///
/// The shared `TesterTrait` logic is inlined here; revisit common extraction when `CommandTester`
/// is ported.
pub struct ApplicationTester {
    application: ApplicationHandle,
    inputs: Vec<String>,
    status_code: Option<i32>,
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
    /// Handles retained before injection so `get_display`/`get_error_output` can read the memory
    /// streams without relying on a `get_stream()` accessor across the ConsoleOutput composition gap.
    output_stream: Option<PhpResource>,
    error_stream: Option<PhpResource>,
    capture_streams_independently: bool,
}

impl ApplicationTester {
    pub fn new(application: ApplicationHandle) -> Self {
        Self {
            application,
            inputs: Vec::new(),
            status_code: None,
            output: None,
            output_stream: None,
            error_stream: None,
            capture_streams_independently: false,
        }
    }

    pub fn set_inputs(&mut self, inputs: Vec<String>) -> &mut Self {
        self.inputs = inputs;
        self
    }

    pub fn run(
        &mut self,
        input: Vec<(PhpMixed, PhpMixed)>,
        options: RunOptions,
    ) -> anyhow::Result<i32> {
        let mut array_input = ArrayInput::new(input, None)?;
        if let Some(interactive) = options.interactive {
            array_input.set_interactive(interactive);
        }
        if !self.inputs.is_empty() {
            array_input.set_stream(Self::create_stream(&self.inputs));
        }

        self.init_output(&options);

        let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(array_input));
        let output = self.output.clone().expect("init_output initializes output");

        let status_code = self.application.run(Some(input), Some(output))?;
        self.status_code = Some(status_code);

        Ok(status_code)
    }

    fn init_output(&mut self, options: &RunOptions) {
        self.capture_streams_independently = options.capture_stderr_separately;

        if !self.capture_streams_independently {
            let stream = shirabe_php_shim::php_fopen_resource("php://memory", "w");
            self.output_stream = Some(stream.clone());
            self.error_stream = None;

            let output = StreamOutput::new(stream, None, None, None)
                .unwrap()
                .expect("php://memory is a valid stream");
            if let Some(decorated) = options.decorated {
                output.set_decorated(decorated);
            }
            if let Some(verbosity) = options.verbosity {
                output.set_verbosity(verbosity);
            }
            self.output = Some(std::rc::Rc::new(std::cell::RefCell::new(output)));
        } else {
            let stdout = shirabe_php_shim::php_fopen_resource("php://memory", "w");
            let stderr = shirabe_php_shim::php_fopen_resource("php://memory", "w");
            self.output_stream = Some(stdout.clone());
            self.error_stream = Some(stderr.clone());

            let mut output =
                ConsoleOutput::new(options.verbosity, options.decorated, None).unwrap();

            let error_output = StreamOutput::new(stderr, None, None, None)
                .unwrap()
                .expect("php://memory is a valid stream");
            error_output.set_formatter(output.get_formatter());
            error_output.set_verbosity(output.get_verbosity());
            error_output.set_decorated(output.is_decorated());

            output.set_error_output(std::rc::Rc::new(std::cell::RefCell::new(error_output)));
            output.__set_stream(stdout);

            self.output = Some(std::rc::Rc::new(std::cell::RefCell::new(output)));
        }
    }

    fn create_stream(inputs: &[String]) -> PhpResource {
        let stream = shirabe_php_shim::php_fopen_resource("php://memory", "r+");
        for input in inputs {
            shirabe_php_shim::fwrite_resource(
                &stream,
                &format!("{}{}", input, shirabe_php_shim::PHP_EOL),
            );
        }
        shirabe_php_shim::rewind(&stream);
        stream
    }

    pub fn get_status_code(&self) -> i32 {
        self.status_code
            .expect("status code not initialized; did you run() before requesting it?")
    }

    pub fn get_display(&self) -> String {
        let stream = self
            .output_stream
            .as_ref()
            .expect("output not initialized; did you run() before requesting the display?");
        shirabe_php_shim::rewind(stream);
        shirabe_php_shim::stream_get_contents(stream).unwrap_or_default()
    }

    pub fn get_error_output(&self) -> String {
        assert!(
            self.capture_streams_independently,
            "The error output is not available when the tester is run without \"capture_stderr_separately\" option set."
        );
        let stream = self
            .error_stream
            .as_ref()
            .expect("error output not initialized; did you run() before requesting it?");
        shirabe_php_shim::rewind(stream);
        shirabe_php_shim::stream_get_contents(stream).unwrap_or_default()
    }
}
