//! ref: composer/src/Composer/Command/SuggestsCommand.php

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::installer::SuggestedPackagesReporter;
use crate::io::IOInterface;
use crate::repository::InstalledRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
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
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;
        let mut composer = crate::command::composer_full_mut(&composer);

        let root_package_handle: crate::package::RootPackageInterfaceHandle =
            composer.get_package().clone();
        let mut installed_repos: Vec<Box<dyn RepositoryInterface>> =
            vec![Box::new(RootPackageRepository::new(root_package_handle))];

        if composer.get_locker().borrow_mut().is_locked() {
            // TODO(phase-b): get_platform_overrides returns IndexMap<String, String>; PlatformRepository::new expects IndexMap<String, PhpMixed>
            let _platform_overrides = composer
                .get_locker()
                .borrow_mut()
                .get_platform_overrides()?;
            let platform_overrides: IndexMap<String, PhpMixed> =
                todo!("convert IndexMap<String, String> to IndexMap<String, PhpMixed>");
            installed_repos.push(Box::new(PlatformRepository::new(
                vec![],
                platform_overrides,
            )?));
            let locked_repo = composer
                .get_locker()
                .borrow_mut()
                .get_locked_repository(!input.get_option("no-dev").as_bool().unwrap_or(false))?;
            installed_repos.push(Box::new(locked_repo));
        } else {
            // TODO(phase-b): Config::get returns PhpMixed; need to coerce to IndexMap<String, PhpMixed>
            let _platform_cfg = composer.get_config().borrow().get("platform");
            let platform_overrides: IndexMap<String, PhpMixed> =
                todo!("extract IndexMap<String, PhpMixed> from PhpMixed config value");
            installed_repos.push(Box::new(PlatformRepository::new(
                vec![],
                platform_overrides,
            )?));
            installed_repos.push(
                composer
                    .get_repository_manager()
                    .borrow()
                    .get_local_repository()
                    .clone_box(),
            );
        }

        let installed_repo = InstalledRepository::new(installed_repos);
        // TODO(phase-b): SuggestedPackagesReporter::new expects std::rc::Rc<std::cell::RefCell<dyn IOInterface>>; self.get_io() returns &mut dyn IOInterface
        let io_box: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            todo!("share IOInterface as Box<dyn IOInterface>");
        let mut reporter = SuggestedPackagesReporter::new(io_box);

        let filter = input.get_argument("packages");
        let mut packages = RepositoryInterface::get_packages(&installed_repo);
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

        if input.get_option("by-suggestion").as_bool().unwrap_or(false) {
            mode = SuggestedPackagesReporter::MODE_BY_SUGGESTION;
        }
        if input.get_option("by-package").as_bool().unwrap_or(false) {
            mode |= SuggestedPackagesReporter::MODE_BY_PACKAGE;
        }
        if input.get_option("list").as_bool().unwrap_or(false) {
            mode = SuggestedPackagesReporter::MODE_LIST;
        }

        let only_dependents_of: Option<crate::package::PackageInterfaceHandle> =
            if empty(&filter) && !input.get_option("all").as_bool().unwrap_or(false) {
                Some(composer.get_package().clone().into())
            } else {
                None
            };

        reporter.output(mode, Some(&installed_repo), only_dependents_of);

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
