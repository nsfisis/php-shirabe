//! ref: composer/src/Composer/Command/SuggestsCommand.php

use crate::command::base_command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::installer::suggested_packages_reporter::SuggestedPackagesReporter;
use crate::io::io_interface::IOInterface;
use crate::repository::installed_repository::InstalledRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::root_package_repository::RootPackageRepository;
use anyhow::Result;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
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
            .set_definition(vec![
                InputOption::new("by-package", None, Some(InputOption::VALUE_NONE), "Groups output by suggesting package (default)", None),
                InputOption::new("by-suggestion", None, Some(InputOption::VALUE_NONE), "Groups output by suggested package", None),
                InputOption::new("all", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Show suggestions from all dependencies, including transitive ones", None),
                InputOption::new("list", None, Some(InputOption::VALUE_NONE), "Show only list of suggested package names", None),
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Exclude suggestions from require-dev packages", None),
                InputArgument::new("packages", Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL), "Packages that you want to list suggestions from.", None),
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

        let mut installed_repos = vec![Box::new(RootPackageRepository::new(
            composer.get_package().clone(),
        ))];

        let locker = composer.get_locker();
        if locker.is_locked() {
            installed_repos.push(Box::new(PlatformRepository::new(
                vec![],
                locker.get_platform_overrides(),
            )));
            installed_repos.push(Box::new(locker.get_locked_repository(
                !input.get_option("no-dev").as_bool().unwrap_or(false),
            )));
        } else {
            installed_repos.push(Box::new(PlatformRepository::new(
                vec![],
                composer.get_config().get("platform"),
            )));
            installed_repos.push(Box::new(
                composer.get_repository_manager().get_local_repository(),
            ));
        }

        let installed_repo = InstalledRepository::new(installed_repos);
        let mut reporter = SuggestedPackagesReporter::new(self.get_io());

        let filter = input.get_argument("packages");
        let mut packages = installed_repo.get_packages();
        packages.push(composer.get_package());
        for package in &packages {
            if !empty(&filter)
                && !in_array(
                    PhpMixed::String(package.get_name().to_string()),
                    &filter,
                    false,
                )
            {
                continue;
            }
            reporter.add_suggestions_from_package(package);
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

        reporter.output(
            mode,
            &installed_repo,
            if empty(&filter) && !input.get_option("all").as_bool().unwrap_or(false) {
                Some(composer.get_package())
            } else {
                None
            },
        );

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
