//! ref: composer/src/Composer/Command/AuditCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, UnexpectedValueException, array_fill_keys, array_merge,
    implode, in_array,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::input::InputOption;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::repository::CanonicalPackagesTrait;
use crate::repository::InstalledRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositorySet;
use crate::repository::RepositoryUtils;

#[derive(Debug)]
pub struct AuditCommand {
    base_command_data: BaseCommandData,
}

impl AuditCommand {
    pub fn new() -> Self {
        let mut command = AuditCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("AuditCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for AuditCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        self.set_name("audit")?;
        self.set_description("Checks for security vulnerability advisories for installed packages");
        self.set_definition(&[
            InputOption::new(
                "no-dev",
                None,
                Some(InputOption::VALUE_NONE),
                "Disables auditing of require-dev packages.",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "format",
                Some(PhpMixed::String("f".to_string())),
                Some(InputOption::VALUE_REQUIRED),
                "Output format. Must be \"table\", \"plain\", \"json\", or \"summary\".",
                Some(PhpMixed::String(Auditor::FORMAT_TABLE.to_string())),
            )
            .unwrap()
            .into(),
            InputOption::new(
                "locked",
                None,
                Some(InputOption::VALUE_NONE),
                "Audit based on the lock file instead of the installed packages.",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "abandoned",
                None,
                Some(InputOption::VALUE_REQUIRED),
                "Behavior on abandoned packages. Must be \"ignore\", \"report\", or \"fail\".",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "ignore-severity",
                None,
                Some(InputOption::VALUE_IS_ARRAY | InputOption::VALUE_REQUIRED),
                "Ignore advisories of a certain severity level.",
                Some(PhpMixed::Array(indexmap::IndexMap::new())),
            )
            .unwrap()
            .into(),
            InputOption::new(
                "ignore-unreachable",
                None,
                Some(InputOption::VALUE_NONE),
                "Ignore repositories that are unreachable or return a non-200 status code.",
                None,
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "The <info>audit</info> command checks for security vulnerability advisories for installed packages.\n\n\
            If you do not want to include dev dependencies in the audit you can omit them with --no-dev\n\n\
            If you want to ignore repositories that are unreachable or return a non-200 status code, use --ignore-unreachable\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#audit"
        );
        Ok(())
    }

    fn execute(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        _output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;
        let packages = self.get_packages(&composer, input.clone())?;

        if packages.is_empty() {
            self.get_io().write_error("No packages - skipping audit.");
            return Ok(0);
        }

        let composer = crate::command::composer_full(&composer);
        let auditor = Auditor;
        let mut repo_set = RepositorySet::new(
            "stable",
            indexmap::IndexMap::new(),
            Vec::new(),
            indexmap::IndexMap::new(),
            indexmap::IndexMap::new(),
            indexmap::IndexMap::new(),
        );
        for repo in composer
            .get_repository_manager()
            .borrow()
            .get_repositories()
        {
            repo_set.add_repository(repo.clone())?;
        }

        let audit_config = AuditConfig::from_config(
            &mut *composer.get_config().borrow_mut(),
            true,
            Auditor::FORMAT_SUMMARY,
        )?;

        let abandoned = input
            .borrow()
            .get_option("abandoned")?
            .as_string()
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

        let mut ignore_severities: indexmap::IndexMap<String, Option<String>> =
            indexmap::IndexMap::new();
        let cli_severities = input.borrow().get_option("ignore-severity")?;
        if let Some(list) = cli_severities.as_list() {
            for sev in list {
                if let Some(s) = sev.as_string() {
                    ignore_severities.insert(s.to_string(), None);
                }
            }
        }
        for (k, v) in audit_config.ignore_severity_for_audit.clone() {
            ignore_severities.insert(k, v);
        }
        let ignore_unreachable = input
            .borrow()
            .get_option("ignore-unreachable")?
            .as_bool()
            .unwrap_or(false)
            || audit_config.ignore_unreachable;

        let audit_format = self.get_audit_format(input, "format")?;
        Ok(auditor
            .audit(
                &mut *self.get_io().borrow_mut(),
                &repo_set,
                packages,
                &audit_format,
                false,
                audit_config.ignore_list_for_audit.clone(),
                &abandoned,
                ignore_severities,
                ignore_unreachable,
                audit_config.ignore_abandoned_for_audit.clone(),
            )?
            .min(255))
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

impl BaseCommand for AuditCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl AuditCommand {
    fn get_packages(
        &self,
        composer: &PartialComposerHandle,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    ) -> Result<Vec<crate::package::PackageInterfaceHandle>> {
        let mut composer = crate::command::composer_full_mut(composer);
        if input
            .borrow()
            .get_option("locked")?
            .as_bool()
            .unwrap_or(false)
        {
            let locker = composer.get_locker().clone();
            let mut locker = locker.borrow_mut();
            if !locker.is_locked() {
                return Err(UnexpectedValueException {
                    message: "Valid composer.json and composer.lock files are required to run this command with --locked".to_string(),
                    code: 0,
                }.into());
            }
            let locked_repo = locker.get_locked_repository(
                !input
                    .borrow()
                    .get_option("no-dev")?
                    .as_bool()
                    .unwrap_or(false),
            )?;
            return locked_repo.borrow_mut().get_canonical_packages();
        }

        let _root_pkg = composer.get_package();
        // TODO(phase-c): InstalledRepository::new expects Vec<Box<dyn RepositoryInterface>>, but
        // get_local_repository returns &dyn InstalledRepositoryInterface. Conversion requires
        // either cloning into a Box or restructuring InstalledRepository constructor.
        let _ = RepositoryUtils::filter_required_packages;
        todo!("audit get_packages non-locked branch needs installed-repo conversion")
    }
}
