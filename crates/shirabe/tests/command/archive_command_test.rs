//! ref: composer/tests/Composer/Test/Command/ArchiveCommandTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::command::BaseCommand;
use shirabe::command::archive_command::ArchiveCommand;
use shirabe::composer::{Composer, PartialComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::dependency_resolver::Transaction;
use shirabe::event_dispatcher::{Callable, EventDispatcherInterface, EventInterface};
use shirabe::package::{
    ArchiveManagerInterface, CompletePackageInterfaceHandle, RootPackageHandle,
};
use shirabe::repository::{
    InstalledArrayRepository, RepositoryInterfaceHandle, RepositoryManagerInterface,
};
use shirabe::util::Platform;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_external_packages::symfony::console::output::buffered_output::BufferedOutput;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::VersionParser;

use crate::config_stub::ConfigStubBuilder;

/// A recorded `ArchiveManager::archive()` call (PHPUnit `->with(...)->willReturn(...)`).
#[derive(Debug, Clone)]
struct ArchiveCall {
    package_name: String,
    format: String,
    target_dir: String,
    file_name: Option<String>,
    ignore_filters: bool,
}

/// Equivalent to a `getMockBuilder(ArchiveManager::class)` mock whose `archive` method is
/// stubbed to record its arguments and return a fixed path.
#[derive(Debug)]
struct ArchiveManagerMock {
    calls: Rc<RefCell<Vec<ArchiveCall>>>,
    return_value: String,
}

impl ArchiveManagerInterface for ArchiveManagerMock {
    fn archive(
        &mut self,
        package: CompletePackageInterfaceHandle,
        format: String,
        target_dir: String,
        file_name: Option<String>,
        ignore_filters: bool,
    ) -> anyhow::Result<String> {
        self.calls.borrow_mut().push(ArchiveCall {
            package_name: package.get_name(),
            format,
            target_dir,
            file_name,
            ignore_filters,
        });
        Ok(self.return_value.clone())
    }
}

/// Equivalent to a `getMockBuilder(EventDispatcher::class)->disableOriginalConstructor()` mock
/// with no behavioral expectations: every dispatch is a no-op.
#[derive(Debug, Default)]
struct NoopEventDispatcher;

impl EventDispatcherInterface for NoopEventDispatcher {
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

fn root_package(name: &str, version: &str) -> RootPackageHandle {
    let normalized = VersionParser.normalize(version, None).unwrap();
    RootPackageHandle::new(name.to_string(), normalized, version.to_string())
}

fn full_composer(composer: Composer) -> PartialComposerHandle {
    PartialComposerHandle::from_rc(Rc::new(RefCell::new(PartialOrFullComposer::Full(composer))))
}

#[test]
fn test_uses_config_from_composer_object() {
    let input: Rc<RefCell<dyn InputInterface>> =
        Rc::new(RefCell::new(ArrayInput::new(vec![], None).unwrap()));
    let output: Rc<RefCell<dyn OutputInterface>> =
        Rc::new(RefCell::new(BufferedOutput::new(None, false, None)));

    let config: Rc<RefCell<Config>> = ConfigStubBuilder::new()
        .with("archive-format", PhpMixed::from("zip"))
        .build_shared();

    let calls = Rc::new(RefCell::new(Vec::new()));
    let manager = ArchiveManagerMock {
        calls: calls.clone(),
        return_value: Platform::get_cwd(false).unwrap(),
    };

    let package = root_package("archive/test", "1.0.0");

    let mut composer = Composer::new();
    composer.set_config(config);
    composer.set_archive_manager(Rc::new(RefCell::new(manager)));
    composer.set_event_dispatcher(Rc::new(RefCell::new(NoopEventDispatcher)));
    composer.set_package(package.into());
    let composer = full_composer(composer);

    // tryComposer()/requireComposer() resolve to the pre-set Composer (PHPUnit overrides both).
    let command = ArchiveCommand::new();
    command.set_composer(composer);

    command.run(input, output).unwrap();

    let calls = calls.borrow();
    assert_eq!(1, calls.len());
    // PHP asserts archive() receives `$package` itself; we check the identity via the package name
    // we set on the Composer.
    assert_eq!("archive/test", calls[0].package_name);
    assert_eq!("zip", calls[0].format);
    assert_eq!(".", calls[0].target_dir);
    assert_eq!(None, calls[0].file_name);
    assert!(!calls[0].ignore_filters);
}

#[test]
fn test_uses_config_from_factory_when_composer_is_not_defined() {
    let input: Rc<RefCell<dyn InputInterface>> =
        Rc::new(RefCell::new(ArrayInput::new(vec![], None).unwrap()));
    let output: Rc<RefCell<dyn OutputInterface>> =
        Rc::new(RefCell::new(BufferedOutput::new(None, false, None)));

    // tryComposer() returns null (no Composer set), so execute() builds the Config via the Factory
    // and `archive` is stubbed (PHPUnit overrides initialize/tryComposer/archive).
    let command = ArchiveCommand::new();
    command.__test_skip_initialize();
    command.__test_stub_archive(0);

    assert_eq!(0, command.run(input, output).unwrap());

    let calls = command.__test_archive_calls();
    assert_eq!(1, calls.len());
    // PHP additionally asserts arg2 === Factory::createConfig() (config-resolution path identity).
    // Not reproduced: capturing arg2 needs a src-side hook on ArchiveCommand's ArchiveCallRecord,
    // and Config has no identity equality. The factory path is only indirectly evidenced here by
    // had_composer == false plus format == "tar" (the archive-format default when no Composer config).
    assert_eq!(None, calls[0].package_name);
    assert_eq!(None, calls[0].version);
    assert_eq!("tar", calls[0].format);
    assert_eq!(".", calls[0].dest);
    assert_eq!(None, calls[0].file_name);
    assert!(!calls[0].ignore_filters);
    assert!(!calls[0].had_composer);
}

/// Equivalent to a `getMockBuilder(RepositoryManager::class)->disableOriginalConstructor()` mock:
/// `getLocalRepository` returns the installed repository, `getRepositories` returns `[]`.
#[derive(Debug)]
struct RepositoryManagerMock {
    local: RepositoryInterfaceHandle,
    repositories: Vec<RepositoryInterfaceHandle>,
}

impl RepositoryManagerInterface for RepositoryManagerMock {
    fn get_local_repository(&self) -> RepositoryInterfaceHandle {
        self.local.clone()
    }

    fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle> {
        &self.repositories
    }

    fn create_repository(
        &self,
        _type: &str,
        _config: IndexMap<String, PhpMixed>,
        _name: Option<&str>,
    ) -> anyhow::Result<RepositoryInterfaceHandle> {
        unreachable!("ArchiveCommand does not create repositories")
    }

    fn add_repository(&mut self, _repository: RepositoryInterfaceHandle) {
        unreachable!("ArchiveCommand does not add repositories")
    }

    fn set_local_repository(&mut self, _repository: RepositoryInterfaceHandle) {
        unreachable!("ArchiveCommand does not set the local repository")
    }
}

#[test]
fn test_uses_config_from_composer_object_with_package_name() {
    let input: Rc<RefCell<dyn InputInterface>> = Rc::new(RefCell::new(
        ArrayInput::new(
            vec![(PhpMixed::from("package"), PhpMixed::from("foo/bar"))],
            None,
        )
        .unwrap(),
    ));
    let output: Rc<RefCell<dyn OutputInterface>> =
        Rc::new(RefCell::new(BufferedOutput::new(None, false, None)));

    let config: Rc<RefCell<Config>> = ConfigStubBuilder::new()
        .with("archive-format", PhpMixed::from("zip"))
        .build_shared();

    let calls = Rc::new(RefCell::new(Vec::new()));
    let manager = ArchiveManagerMock {
        calls: calls.clone(),
        return_value: Platform::get_cwd(false).unwrap(),
    };

    let package = root_package("foo/bar", "1.0.0");

    // The local repository resolves `foo/bar` (PHPUnit mocks InstalledRepositoryInterface::loadPackages).
    let installed =
        InstalledArrayRepository::new_with_packages(vec![package.clone().into()]).unwrap();
    let repository_manager = RepositoryManagerMock {
        local: RepositoryInterfaceHandle::new(installed),
        repositories: vec![],
    };

    let mut composer = Composer::new();
    composer.set_config(config);
    composer.set_archive_manager(Rc::new(RefCell::new(manager)));
    composer.set_event_dispatcher(Rc::new(RefCell::new(NoopEventDispatcher)));
    composer.set_package(package.into());
    composer.set_repository_manager(Rc::new(RefCell::new(repository_manager)));
    let composer = full_composer(composer);

    let command = ArchiveCommand::new();
    command.set_composer(composer);

    command.run(input, output).unwrap();

    let calls = calls.borrow();
    assert_eq!(1, calls.len());
    assert_eq!("foo/bar", calls[0].package_name);
    assert_eq!("zip", calls[0].format);
    assert_eq!(".", calls[0].target_dir);
    assert_eq!(None, calls[0].file_name);
    assert!(!calls[0].ignore_filters);
}
