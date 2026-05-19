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
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
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
            .set_definition(&[
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables auditing of require-dev packages.", None).unwrap().into(),
                InputOption::new("format", Some(PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Output format. Must be \"table\", \"plain\", \"json\", or \"summary\".", Some(PhpMixed::String(Auditor::FORMAT_TABLE.to_string()))).unwrap().into(),
                InputOption::new("locked", None, Some(InputOption::VALUE_NONE), "Audit based on the lock file instead of the installed packages.", None).unwrap().into(),
                InputOption::new("abandoned", None, Some(InputOption::VALUE_REQUIRED), "Behavior on abandoned packages. Must be \"ignore\", \"report\", or \"fail\".", None).unwrap().into(),
                InputOption::new("ignore-severity", None, Some(InputOption::VALUE_IS_ARRAY | InputOption::VALUE_REQUIRED), "Ignore advisories of a certain severity level.", Some(PhpMixed::Array(indexmap::IndexMap::new()))).unwrap().into(),
                InputOption::new("ignore-unreachable", None, Some(InputOption::VALUE_NONE), "Ignore repositories that are unreachable or return a non-200 status code.", None).unwrap().into(),
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

        let auditor = Auditor;
        let mut repo_set = RepositorySet::new(
            "stable",
            indexmap::IndexMap::new(),
            Vec::new(),
            indexmap::IndexMap::new(),
            indexmap::IndexMap::new(),
            indexmap::IndexMap::new(),
        );
        for repo in composer.get_repository_manager().get_repositories() {
            // TODO(phase-b): repositories are shared (PHP class semantics); needs Rc wrapper
            repo_set.add_repository(repo.clone_box())?;
        }

        let audit_config = AuditConfig::from_config(
            &mut *composer.get_config().borrow_mut(),
            true,
            Auditor::FORMAT_SUMMARY,
        )?;

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

        let audit_format = self.get_audit_format(input, "format")?;
        // TODO(phase-b): ignore_severities is PhpMixed; need conversion to IndexMap<String, Option<String>>
        let _ = ignore_severities;
        Ok(auditor
            .audit(
                self.get_io(),
                &repo_set,
                packages,
                &audit_format,
                false,
                audit_config.ignore_list_for_audit.clone(),
                &abandoned,
                indexmap::IndexMap::new(),
                ignore_unreachable,
                audit_config.ignore_abandoned_for_audit.clone(),
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
            return Ok(CanonicalPackagesTrait::get_packages(
                &locker.get_locked_repository(
                    !input.get_option("no-dev").as_bool().unwrap_or(false),
                )?,
            ));
        }

        let _root_pkg = composer.get_package();
        // TODO(phase-b): InstalledRepository::new expects Vec<Box<dyn RepositoryInterface>>, but
        // get_local_repository returns &dyn InstalledRepositoryInterface. Conversion requires
        // either cloning into a Box or restructuring InstalledRepository constructor.
        let _ = RepositoryUtils::filter_required_packages;
        todo!("audit get_packages non-locked branch needs installed-repo conversion")
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
