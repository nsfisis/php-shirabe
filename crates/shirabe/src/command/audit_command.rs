//! ref: composer/src/Composer/Command/AuditCommand.php

use crate::advisory::audit_config::AuditConfig;
use crate::advisory::auditor::Auditor;
use crate::command::base_command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::Composer;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::repository::canonical_packages_trait::CanonicalPackagesTrait;
use crate::repository::installed_repository::InstalledRepository;
use crate::repository::repository_interface::RepositoryInterface;
use crate::repository::repository_set::RepositorySet;
use crate::repository::repository_utils::RepositoryUtils;
use anyhow::Result;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, UnexpectedValueException, array_fill_keys, array_merge,
    implode, in_array,
};

#[derive(Debug)]
pub struct AuditCommand {
    base_command_data: BaseCommandData,
}

impl AuditCommand {
    pub fn configure(&mut self) {
        self
            .set_name("audit")
            .set_description("Checks for security vulnerability advisories for installed packages")
            .set_definition(vec![
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables auditing of require-dev packages.", None),
                InputOption::new("format", Some(PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Output format. Must be \"table\", \"plain\", \"json\", or \"summary\".", Some(PhpMixed::String(Auditor::FORMAT_TABLE.to_string()))),
                InputOption::new("locked", None, Some(InputOption::VALUE_NONE), "Audit based on the lock file instead of the installed packages.", None),
                InputOption::new("abandoned", None, Some(InputOption::VALUE_REQUIRED), "Behavior on abandoned packages. Must be \"ignore\", \"report\", or \"fail\".", None),
                InputOption::new("ignore-severity", None, Some(InputOption::VALUE_IS_ARRAY | InputOption::VALUE_REQUIRED), "Ignore advisories of a certain severity level.", Some(PhpMixed::Array(indexmap::IndexMap::new()))),
                InputOption::new("ignore-unreachable", None, Some(InputOption::VALUE_NONE), "Ignore repositories that are unreachable or return a non-200 status code.", None),
            ])
            .set_help(
                "The <info>audit</info> command checks for security vulnerability advisories for installed packages.\n\n\
                If you do not want to include dev dependencies in the audit you can omit them with --no-dev\n\n\
                If you want to ignore repositories that are unreachable or return a non-200 status code, use --ignore-unreachable\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#audit"
            );
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;
        let packages = self.get_packages(&composer, input)?;

        if packages.is_empty() {
            self.get_io().write_error("No packages - skipping audit.");
            return Ok(0);
        }

        let auditor = Auditor::new();
        let mut repo_set = RepositorySet::new();
        for repo in composer.get_repository_manager().get_repositories() {
            repo_set.add_repository(repo);
        }

        let audit_config =
            AuditConfig::from_config(composer.get_config(), true, Auditor::FORMAT_SUMMARY)?;

        let abandoned = input
            .get_option("abandoned")
            .as_string_opt()
            .map(|s| s.to_string());
        if abandoned.is_some()
            && !in_array(
                PhpMixed::String(abandoned.clone().unwrap()),
                &PhpMixed::from(Auditor::ABANDONEDS.to_vec()),
                true,
            )
        {
            return Err(InvalidArgumentException {
                message: format!(
                    "--abandoned must be one of {}.",
                    implode(
                        ", ",
                        &Auditor::ABANDONEDS
                            .iter()
                            .map(|s| s.to_string())
                            .collect::<Vec<_>>()
                    )
                ),
                code: 0,
            }
            .into());
        }

        let abandoned = abandoned.unwrap_or_else(|| audit_config.audit_abandoned.clone());

        let ignore_severities = array_merge(
            array_fill_keys(input.get_option("ignore-severity"), PhpMixed::Null),
            PhpMixed::from(audit_config.ignore_severity_for_audit.clone()),
        );
        let ignore_unreachable = input
            .get_option("ignore-unreachable")
            .as_bool()
            .unwrap_or(false)
            || audit_config.ignore_unreachable;

        Ok(auditor
            .audit(
                self.get_io(),
                &repo_set,
                &packages,
                &self.get_audit_format(input, "format"),
                false,
                &audit_config.ignore_list_for_audit,
                &abandoned,
                &ignore_severities,
                ignore_unreachable,
                &audit_config.ignore_abandoned_for_audit,
            )?
            .min(255))
    }

    fn get_packages(
        &self,
        composer: &Composer,
        input: &dyn InputInterface,
    ) -> Result<Vec<Box<dyn PackageInterface>>> {
        if input.get_option("locked").as_bool().unwrap_or(false) {
            if !composer.get_locker().is_locked() {
                return Err(UnexpectedValueException {
                    message: "Valid composer.json and composer.lock files are required to run this command with --locked".to_string(),
                    code: 0,
                }.into());
            }
            let locker = composer.get_locker();
            return Ok(locker
                .get_locked_repository(!input.get_option("no-dev").as_bool().unwrap_or(false))?
                .get_packages());
        }

        let root_pkg = composer.get_package();
        let installed_repo = InstalledRepository::new(vec![
            composer.get_repository_manager().get_local_repository(),
        ]);

        if input.get_option("no-dev").as_bool().unwrap_or(false) {
            return Ok(RepositoryUtils::filter_required_packages(
                installed_repo.get_packages(),
                root_pkg,
            ));
        }

        Ok(installed_repo.get_packages())
    }
}

impl HasBaseCommandData for AuditCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
