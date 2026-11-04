//! ref: composer/tests/Composer/Test/Command/ArchiveCommandTest.php sha1:c90d9d37e73bef49fceaf4b790f25f3b496c8395

use crate::config_stub::ConfigStubBuilder;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::command::BaseCommand;
use shirabe::command::archive_command::ArchiveCommand;
use shirabe::composer::{Composer, PartialComposerHandle, PartialOrFullComposer};
use shirabe::config::Config;
use shirabe::dependency_resolver::AnyTransaction;
use shirabe::event_dispatcher::{Callable, EventDispatcherInterface, EventInterface};
use shirabe::package::{
    ArchiveManagerInterface, CompletePackageInterfaceHandle, RootPackageHandle,
};
use shirabe::repository::{
    InstalledArrayRepository, RepositoryInterfaceHandle, RepositoryManagerInterface,
};
use shirabe::util::Platform;
use shirabe_php_shim::{PhpMixed, bconcat, path_to_bytes};
use shirabe_semver::VersionParser;
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{ArrayInput, InputInterface, InputValue, ParameterName};
use shirabe_symfony_console::output::{BufferedOutput, OutputInterface};

// PHP mocks `Composer\Package\Archiver\ArchiveManager` with
// getMockBuilder(...)->disableOriginalConstructor().
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_archive_manager {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub ArchiveManager {}
        #[async_trait::async_trait(?Send)]
        impl ArchiveManagerInterface for ArchiveManager {
            async fn archive(
                &mut self,
                package: CompletePackageInterfaceHandle,
                format: bstr::BString,
                target_dir: std::path::PathBuf,
                file_name: Option<bstr::BString>,
                ignore_filters: bool,
            ) -> anyhow::Result<std::path::PathBuf>;
        }
    }
}
use mock_archive_manager::MockArchiveManager;

// PHP mocks `Composer\Repository\RepositoryManager` with
// getMockBuilder(...)->disableOriginalConstructor().
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_repository_manager {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub RepositoryManager {}
        impl RepositoryManagerInterface for RepositoryManager {
            fn get_local_repository(&self) -> RepositoryInterfaceHandle;
            fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle>;
            fn create_repository<'a>(
                &self,
                r#type: &BStr,
                config: IndexMap<BString, PhpMixed>,
                name: Option<&'a bstr::BStr>,
            ) -> anyhow::Result<RepositoryInterfaceHandle>;
            fn add_repository(&mut self, repository: RepositoryInterfaceHandle);
            fn set_local_repository(&mut self, repository: RepositoryInterfaceHandle);
        }
    }
}
use mock_repository_manager::MockRepositoryManager;

// PHP mocks `Composer\EventDispatcher\EventDispatcher` with
// getMockBuilder(...)->disableOriginalConstructor() and no method stubs.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_event_dispatcher {
    use super::*;

    mockall::mock! {
        #[derive(Debug)]
        pub EventDispatcher {}
        impl EventDispatcherInterface for EventDispatcher {
            fn dispatch<'a>(
                &mut self,
                event_name: Option<&'a bstr::BStr>,
                event: Option<std::rc::Rc<std::cell::RefCell<dyn EventInterface>>>,
            ) -> anyhow::Result<i64>;
            fn dispatch_script(
                &mut self,
                event_name: &bstr::BStr,
                dev_mode: bool,
                additional_args: Vec<bstr::BString>,
                flags: shirabe::event_dispatcher::EventFlags,
            ) -> anyhow::Result<i64>;
            fn dispatch_installer_event(
                &mut self,
                event_name: &bstr::BStr,
                dev_mode: bool,
                execute_operations: bool,
                transaction: std::rc::Rc<AnyTransaction>,
            ) -> anyhow::Result<i64>;
            fn add_listener(&mut self, event_name: &bstr::BStr, listener: Callable, priority: i64);
            fn add_subscriber<'a>(
                &mut self,
                subscriber: &'a dyn shirabe::event_dispatcher::EventSubscriberInterface,
            ) -> anyhow::Result<()>;
            fn remove_listener<'a>(&mut self, listener: &'a shirabe_php_rpc::PhpObjHandle);
            fn has_event_listeners(&mut self, event: &dyn EventInterface) -> bool;
        }
    }
}
use mock_event_dispatcher::MockEventDispatcher;

/// A `getMockBuilder(EventDispatcher::class)->disableOriginalConstructor()` mock with no
/// behavioral expectations: the dispatch methods ArchiveCommand calls are permissive no-ops.
fn noop_event_dispatcher() -> MockEventDispatcher {
    let mut event_dispatcher = MockEventDispatcher::new();
    event_dispatcher.expect_dispatch().returning(|_, _| Ok(0));
    event_dispatcher
        .expect_dispatch_script()
        .returning(|_, _, _, _| Ok(0));
    event_dispatcher
}

fn root_package(name: &BStr, version: &BStr) -> RootPackageHandle {
    let normalized = VersionParser.normalize(version, None).unwrap();
    RootPackageHandle::new(bconcat!(name), normalized, bconcat!(version))
}

fn full_composer(composer: Composer) -> PartialComposerHandle {
    PartialComposerHandle::from_rc(std::rc::Rc::new(std::cell::RefCell::new(
        PartialOrFullComposer::Full(composer),
    )))
}

#[test]
fn test_uses_config_from_composer_object() {
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(ArrayInput::new(vec![], None).unwrap()),
    );
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferedOutput::new(None, false, None)),
    );

    let config: std::rc::Rc<std::cell::RefCell<Config>> = ConfigStubBuilder::new()
        .with("archive-format", PhpMixed::from("zip"))
        .build_shared();

    let cwd = Platform::get_cwd(false).unwrap();
    let mut manager = MockArchiveManager::new();
    // PHP: ->expects($this->once())->method('archive')
    //        ->with($package, 'zip', '.', null, false)->willReturn(Platform::getCwd()).
    // PHP asserts archive() receives `$package` itself; we check the identity via the package name
    // we set on the Composer.
    manager
        .expect_archive()
        .times(1)
        .withf(|package, format, target_dir, file_name, ignore_filters| {
            package.get_name() == "archive/test"
                && format == "zip"
                && path_to_bytes(target_dir) == b"."
                && file_name.is_none()
                && !*ignore_filters
        })
        .returning(move |_, _, _, _, _| Ok(cwd.clone()));

    let package = root_package(BStr::new("archive/test"), BStr::new("1.0.0"));

    let mut composer = Composer::new();
    composer.set_config(config);
    composer.set_archive_manager(std::rc::Rc::new(std::cell::RefCell::new(manager)));
    composer.set_event_dispatcher(std::rc::Rc::new(std::cell::RefCell::new(
        noop_event_dispatcher(),
    )));
    composer.set_package(package.into());
    let composer = full_composer(composer);

    // tryComposer()/requireComposer() resolve to the pre-set Composer (PHPUnit overrides both).
    let command = ArchiveCommand::new();
    command.set_composer(composer);

    shirabe::util::sync_executor::block_on(command.run(input, output)).unwrap();
}

#[test]
fn test_uses_config_from_factory_when_composer_is_not_defined() {
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(ArrayInput::new(vec![], None).unwrap()),
    );
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferedOutput::new(None, false, None)),
    );

    // tryComposer() returns null (no Composer set), so execute() builds the Config via the Factory
    // and `archive` is stubbed (PHPUnit overrides initialize/tryComposer/archive).
    let command = ArchiveCommand::new();
    command.__test_skip_initialize();
    command.__test_stub_archive(0);

    assert_eq!(
        0,
        shirabe::util::sync_executor::block_on(command.run(input, output)).unwrap()
    );

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

#[test]
fn test_uses_config_from_composer_object_with_package_name() {
    let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(
            ArrayInput::new(
                vec![(ParameterName::of("package"), InputValue::from("foo/bar"))],
                None,
            )
            .unwrap(),
        ));
    let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = std::rc::Rc::new(
        std::cell::RefCell::new(BufferedOutput::new(None, false, None)),
    );

    let config: std::rc::Rc<std::cell::RefCell<Config>> = ConfigStubBuilder::new()
        .with("archive-format", PhpMixed::from("zip"))
        .build_shared();

    let cwd = Platform::get_cwd(false).unwrap();
    let mut manager = MockArchiveManager::new();
    // PHP: ->expects($this->once())->method('archive')
    //        ->with($package, 'zip', '.', null, false)->willReturn(Platform::getCwd()).
    manager
        .expect_archive()
        .times(1)
        .withf(|package, format, target_dir, file_name, ignore_filters| {
            package.get_name() == "foo/bar"
                && format == "zip"
                && path_to_bytes(target_dir) == b"."
                && file_name.is_none()
                && !*ignore_filters
        })
        .returning(move |_, _, _, _, _| Ok(cwd.clone()));

    let package = root_package(BStr::new("foo/bar"), BStr::new("1.0.0"));

    let mut repository_manager = MockRepositoryManager::new();
    // PHP: ->expects($this->once())->method('getLocalRepository')->willReturn($installedRepository).
    // The local repository resolves `foo/bar` (PHPUnit mocks InstalledRepositoryInterface::loadPackages).
    // Built inside the closure so it captures nothing (mockall requires `Send` closures).
    repository_manager
        .expect_get_local_repository()
        .times(1)
        .returning(|| {
            let package = root_package(BStr::new("foo/bar"), BStr::new("1.0.0"));
            let installed =
                InstalledArrayRepository::new_with_packages(vec![package.into()]).unwrap();
            RepositoryInterfaceHandle::new(installed)
        });
    // PHP: ->expects($this->once())->method('getRepositories')->willReturn([]).
    repository_manager
        .expect_get_repositories()
        .times(1)
        .return_const(Vec::new());

    let mut composer = Composer::new();
    composer.set_config(config);
    composer.set_archive_manager(std::rc::Rc::new(std::cell::RefCell::new(manager)));
    composer.set_event_dispatcher(std::rc::Rc::new(std::cell::RefCell::new(
        noop_event_dispatcher(),
    )));
    composer.set_package(package.into());
    composer.set_repository_manager(std::rc::Rc::new(std::cell::RefCell::new(
        repository_manager,
    )));
    let composer = full_composer(composer);

    let command = ArchiveCommand::new();
    command.set_composer(composer);

    shirabe::util::sync_executor::block_on(command.run(input, output)).unwrap();
}
