//! ref: composer/src/Composer/Command/SuggestsCommand.php

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::installer::SuggestedPackagesReporter;
use crate::io::IOInterface;
use crate::repository::InstalledRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::RootPackageRepository;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{PhpMixed, empty, in_array};

#[derive(Debug)]
pub struct SuggestsCommand {
    base_command_data: BaseCommandData,
}

impl SuggestsCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_installed_package() for `packages` argument
        self
            .set_name("suggests")
            .set_description("Shows package suggestions")
            .set_definition(&[
                InputOption::new("by-package", None, Some(InputOption::VALUE_NONE), "Groups output by suggesting package (default)", None).unwrap().into(),
        InputOption::new("by-suggestion", None, Some(InputOption::VALUE_NONE), "Groups output by suggested package", None).unwrap().into(),
        InputOption::new("all", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Show suggestions from all dependencies, including transitive ones", None).unwrap().into(),
        InputOption::new("list", None, Some(InputOption::VALUE_NONE), "Show only list of suggested package names", None).unwrap().into(),
        InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Exclude suggestions from require-dev packages", None).unwrap().into(),
        InputArgument::new("packages", Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL), "Packages that you want to list suggestions from.", None).unwrap().into(),
            ])
            .set_help(
                "\nThe <info>%command.name%</info> command shows a sorted list of suggested packages.\n\nRead more at https://getcomposer.org/doc/03-cli.md#suggests",
            );
    }

    pub fn execute(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
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
                    .get_option("no-dev")
                    .as_bool()
                    .unwrap_or(false),
            )?;
            installed_repos.push(locked_repo.into());
        } else {
            // TODO(phase-b): Config::get returns PhpMixed; need to coerce to IndexMap<String, PhpMixed>
            let _platform_cfg = composer.get_config().borrow().get("platform");
            let platform_overrides: IndexMap<String, PhpMixed> =
                todo!("extract IndexMap<String, PhpMixed> from PhpMixed config value");
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

        let filter = input.borrow().get_argument("packages");
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
            .get_option("by-suggestion")
            .as_bool()
            .unwrap_or(false)
        {
            mode = SuggestedPackagesReporter::MODE_BY_SUGGESTION;
        }
        if input
            .borrow()
            .get_option("by-package")
            .as_bool()
            .unwrap_or(false)
        {
            mode |= SuggestedPackagesReporter::MODE_BY_PACKAGE;
        }
        if input.borrow().get_option("list").as_bool().unwrap_or(false) {
            mode = SuggestedPackagesReporter::MODE_LIST;
        }

        let only_dependents_of: Option<crate::package::PackageInterfaceHandle> =
            if empty(&filter) && !input.borrow().get_option("all").as_bool().unwrap_or(false) {
                Some(composer.get_package().clone().into())
            } else {
                None
            };

        reporter.output(mode, Some(&mut installed_repo), only_dependents_of)?;

        Ok(0)
    }
}

impl HasBaseCommandData for SuggestsCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
