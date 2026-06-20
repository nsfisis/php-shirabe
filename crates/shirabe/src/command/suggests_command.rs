//! ref: composer/src/Composer/Command/SuggestsCommand.php

use crate::advisory::AuditConfig;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::installer::SuggestedPackagesReporter;
use crate::io::IOInterface;
use crate::repository::InstalledRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::RootPackageRepository;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{PhpMixed, empty, in_array};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct SuggestsCommand {
    base_command_data: BaseCommandData,
}

impl Default for SuggestsCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl SuggestsCommand {
    pub fn new() -> Self {
        let mut command = SuggestsCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("SuggestsCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for SuggestsCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        // TODO(cli-completion): suggest_installed_package() for `packages` argument
        self.set_name("suggests")?;
        self.set_description("Shows package suggestions");
        self.set_definition(&[
            InputOption::new(
                "by-package",
                None,
                Some(InputOption::VALUE_NONE),
                "Groups output by suggesting package (default)",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "by-suggestion",
                None,
                Some(InputOption::VALUE_NONE),
                "Groups output by suggested package",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "all",
                Some(PhpMixed::String("a".to_string())),
                Some(InputOption::VALUE_NONE),
                "Show suggestions from all dependencies, including transitive ones",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "list",
                None,
                Some(InputOption::VALUE_NONE),
                "Show only list of suggested package names",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "no-dev",
                None,
                Some(InputOption::VALUE_NONE),
                "Exclude suggestions from require-dev packages",
                None,
            )
            .unwrap()
            .into(),
            InputArgument::new(
                "packages",
                Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
                "Packages that you want to list suggestions from.",
                None,
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "\nThe <info>%command.name%</info> command shows a sorted list of suggested packages.\n\nRead more at https://getcomposer.org/doc/03-cli.md#suggests",
        );
        Ok(())
    }

    fn execute(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        _output: Rc<RefCell<dyn OutputInterface>>,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;
        let mut composer = crate::command::composer_full_mut(&composer);

        let mut installed_repos: Vec<RepositoryInterfaceHandle> =
            vec![RepositoryInterfaceHandle::new(RootPackageRepository::new(
                crate::package::RootPackageInterfaceHandle::dup(composer.get_package()),
            ))];

        if composer.get_locker().borrow_mut().is_locked() {
            let platform_overrides = composer
                .get_locker()
                .borrow_mut()
                .get_platform_overrides()?;
            let platform_overrides: IndexMap<String, PhpMixed> = platform_overrides
                .into_iter()
                .map(|(k, v)| (k, PhpMixed::String(v)))
                .collect();
            installed_repos.push(RepositoryInterfaceHandle::new(PlatformRepository::new(
                vec![],
                platform_overrides,
            )?));
            let locked_repo = composer.get_locker().borrow_mut().get_locked_repository(
                !input
                    .borrow()
                    .get_option("no-dev")?
                    .as_bool()
                    .unwrap_or(false),
            )?;
            installed_repos.push(locked_repo.into());
        } else {
            let platform_cfg = composer.get_config().borrow().get("platform");
            let platform_overrides: IndexMap<String, PhpMixed> = platform_cfg
                .as_array()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            installed_repos.push(RepositoryInterfaceHandle::new(PlatformRepository::new(
                vec![],
                platform_overrides,
            )?));
            installed_repos.push(
                composer
                    .get_repository_manager()
                    .borrow()
                    .get_local_repository(),
            );
        }

        let mut installed_repo = InstalledRepository::new(installed_repos);
        let mut reporter = SuggestedPackagesReporter::new(self.get_io().clone());

        let filter = input.borrow().get_argument("packages")?;
        let mut packages = RepositoryInterface::get_packages(&mut installed_repo)?;
        let root_pkg_as_base: crate::package::BasePackageHandle =
            composer.get_package().clone().into();
        packages.push(root_pkg_as_base);
        for package in &packages {
            if !empty(&filter) && !in_array(PhpMixed::String(package.get_name()), &filter, false) {
                continue;
            }
            reporter.add_suggestions_from_package(package.clone());
        }

        let mut mode = SuggestedPackagesReporter::MODE_BY_PACKAGE;

        if input
            .borrow()
            .get_option("by-suggestion")?
            .as_bool()
            .unwrap_or(false)
        {
            mode = SuggestedPackagesReporter::MODE_BY_SUGGESTION;
        }
        if input
            .borrow()
            .get_option("by-package")?
            .as_bool()
            .unwrap_or(false)
        {
            mode |= SuggestedPackagesReporter::MODE_BY_PACKAGE;
        }
        if input
            .borrow()
            .get_option("list")?
            .as_bool()
            .unwrap_or(false)
        {
            mode = SuggestedPackagesReporter::MODE_LIST;
        }

        let only_dependents_of: Option<crate::package::PackageInterfaceHandle> =
            if empty(&filter) && !input.borrow().get_option("all")?.as_bool().unwrap_or(false) {
                Some(composer.get_package().clone().into())
            } else {
                None
            };

        reporter.output(mode, Some(&mut installed_repo), only_dependents_of)?;

        Ok(0)
    }

    fn initialize(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for SuggestsCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
