//! ref: composer/src/Composer/Command/CheckPlatformReqsCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::console::input::{InputOption, SuggestedValues};
use crate::io::IOInterfaceWrite;
use crate::json::JsonFile;
use crate::package::Link;
use crate::repository::{
    InstalledRepository, PlatformRepository, RepositoryInterface, RootPackageRepository,
};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, array_merge_map, bconcat, impl_php_class, json_bytes, rtrim, strip_tags,
};
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName, InputValue};
use shirabe_symfony_console::output::OutputInterface;

struct CheckResult {
    platform_package: BString,
    version: BString,
    link: Option<Link>,
    status: BString,
    provider: BString,
}

/// Shape of one entry of the `check-platform-reqs --format=json` output.
#[derive(Debug, serde::Serialize)]
struct PlatformRequirementJson {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    #[serde(serialize_with = "json_bytes::str")]
    version: BString,
    #[serde(serialize_with = "json_bytes::str")]
    status: BString,
    failed_requirement: Option<FailedRequirementJson>,
    #[serde(serialize_with = "json_bytes::option_str")]
    provider: Option<BString>,
}

#[derive(Debug, serde::Serialize)]
struct FailedRequirementJson {
    #[serde(serialize_with = "json_bytes::str")]
    source: BString,
    #[serde(serialize_with = "json_bytes::str")]
    r#type: BString,
    #[serde(serialize_with = "json_bytes::str")]
    target: BString,
    #[serde(serialize_with = "json_bytes::str")]
    constraint: BString,
}

#[derive(Debug)]
pub struct CheckPlatformReqsCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(
    CheckPlatformReqsCommand,
    r"Composer\Command\CheckPlatformReqsCommand"
);

impl Default for CheckPlatformReqsCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckPlatformReqsCommand {
    pub fn new() -> Self {
        let command = CheckPlatformReqsCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("CheckPlatformReqsCommand::configure uses static, valid metadata");
        command
    }

    fn print_table(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        results: &[CheckResult],
        format: &BStr,
    ) -> anyhow::Result<()> {
        let io = self.get_io();

        if format == "json" {
            let rows: Vec<PlatformRequirementJson> = results
                .iter()
                .map(|result| PlatformRequirementJson {
                    name: result.platform_package.clone(),
                    version: result.version.clone(),
                    status: strip_tags(&result.status),
                    failed_requirement: result.link.as_ref().map(|link| FailedRequirementJson {
                        source: BString::from(link.get_source()),
                        r#type: BString::from(link.get_description()),
                        target: BString::from(link.get_target()),
                        constraint: BString::from(link.get_pretty_constraint()),
                    }),
                    provider: if result.provider.is_empty() {
                        None
                    } else {
                        Some(strip_tags(&result.provider))
                    },
                })
                .collect();

            io.write(&JsonFile::encode(&rows)?);
        } else {
            let rows: Vec<PhpMixed> = results
                .iter()
                .map(|result| {
                    PhpMixed::List(vec![
                        PhpMixed::string(result.platform_package.clone()),
                        PhpMixed::string(result.version.clone()),
                        match &result.link {
                            Some(link) => PhpMixed::string(link.to_php_string()),
                            None => PhpMixed::string(String::new()),
                        },
                        if let Some(link) = &result.link {
                            PhpMixed::string(bconcat!(
                                link.get_source(),
                                " ",
                                link.get_description(),
                                " ",
                                link.get_target(),
                                " (",
                                link.get_pretty_constraint(),
                                ")"
                            ))
                        } else {
                            PhpMixed::string(String::new())
                        },
                        PhpMixed::string(rtrim(bconcat!(result.status, " ", result.provider))),
                    ])
                })
                .collect();

            self.render_table(rows, output)?;
        }

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl Command for CheckPlatformReqsCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("check-platform-reqs".into())?;
        self.set_description("Check that platform requirements are satisfied".into());
        self.set_definition(&[
            InputOption::new(
                "no-dev",
                None,
                Some(InputOption::VALUE_NONE),
                "Disables checking of require-dev packages requirements.",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "lock",
                None,
                Some(InputOption::VALUE_NONE),
                "Checks requirements only from the lock file, not from installed packages.",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new6(
                "format",
                Some("f".into()),
                Some(InputOption::VALUE_REQUIRED),
                "Format of the output: text or json",
                Some(InputValue::from("text")),
                SuggestedValues::List(vec![BString::from("json"), BString::from("text")]),
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "Checks that your PHP and extensions versions match the platform requirements of the installed packages.\n\n\
            Unlike update/install, this command will ignore config.platform settings and check the real platform packages so you can be certain you have the required platform dependencies.\n\n\
            <info>shirabe check-platform-reqs</info>\n\n".into()
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer);
        let io = self.get_io();

        let no_dev = input
            .borrow()
            .get_option("no-dev")?
            .as_bool()
            .unwrap_or(false);

        let mut requires: IndexMap<BString, Vec<Link>> = IndexMap::new();
        let mut remove_packages: Vec<BString> = vec![];

        let installed_repo_base: crate::repository::RepositoryInterfaceHandle = if input
            .borrow()
            .get_option("lock")?
            .as_bool()
            .unwrap_or(false)
        {
            io.write_error(format!(
                "<info>Checking {}platform requirements using the lock file</info>",
                if no_dev { "non-dev " } else { "" }
            ));
            composer
                .get_locker()
                .borrow_mut()
                .get_locked_repository(!no_dev)?
                .into()
        } else {
            let repository_manager = composer.get_repository_manager().clone();
            let repository_manager = repository_manager.borrow();
            let local_repo = repository_manager.get_local_repository();
            if local_repo.get_packages()?.is_empty() {
                io.write_error(format!(
                    "<warning>No vendor dir present, checking {}platform requirements from the lock file</warning>",
                    if no_dev { "non-dev " } else { "" }
                ));
                composer
                    .get_locker()
                    .borrow_mut()
                    .get_locked_repository(!no_dev)?
                    .into()
            } else {
                if no_dev {
                    remove_packages = local_repo.get_dev_package_names();
                }
                io.write_error(format!(
                    "<info>Checking {}platform requirements for packages in the vendor dir</info>",
                    if no_dev { "non-dev " } else { "" }
                ));
                local_repo
            }
        };

        if !no_dev {
            for (require, link) in composer.get_package().get_dev_requires().iter() {
                requires
                    .entry(require.clone())
                    .or_default()
                    .push(link.clone());
            }
        }

        let root_pkg_repo = RootPackageRepository::new(
            crate::package::RootPackageInterfaceHandle::dup(composer.get_package()),
        );
        let mut installed_repo = InstalledRepository::new(vec![
            installed_repo_base,
            crate::repository::RepositoryInterfaceHandle::new(root_pkg_repo),
        ]);

        for package in installed_repo.get_packages()? {
            if remove_packages.contains(&package.get_name()) {
                continue;
            }
            for (require, link) in package.get_requires().iter() {
                requires
                    .entry(require.clone())
                    .or_default()
                    .push(link.clone());
            }
        }

        let mut requires_sorted: Vec<(BString, Vec<Link>)> = requires.into_iter().collect();
        requires_sorted.sort_by(|a, b| a.0.cmp(&b.0));

        installed_repo.add_repository(crate::repository::RepositoryInterfaceHandle::new(
            PlatformRepository::new(vec![], indexmap::IndexMap::new(), None, None)?,
        ));
        let installed_repo_with_platform = installed_repo;

        let mut results: Vec<CheckResult> = vec![];
        let mut exit_code = 0;

        'requirements: for (require, links) in &requires_sorted {
            if PlatformRepository::is_platform_package(require) {
                let candidates = installed_repo_with_platform
                    .find_packages_with_replacers_and_providers(bstr::BStr::new(require), None)?;
                if !candidates.is_empty() {
                    let mut req_results: Vec<CheckResult> = vec![];
                    'candidates: for candidate in &candidates {
                        let candidate_constraint: Option<AnyConstraint> =
                            if candidate.get_name() == *require {
                                let c = SimpleConstraint::new(
                                    "=",
                                    candidate.get_version(),
                                    Some(candidate.get_pretty_version()),
                                );
                                Some(c.into())
                            } else {
                                let mut found: Option<AnyConstraint> = None;
                                let provides_and_replaces = array_merge_map(
                                    (*candidate.get_provides()).clone(),
                                    (*candidate.get_replaces()).clone(),
                                );
                                for (_, link) in &provides_and_replaces {
                                    if link.get_target() == require {
                                        found = Some(link.get_constraint().clone());
                                        break;
                                    }
                                }
                                found
                            };

                        let candidate_constraint = match candidate_constraint {
                            Some(c) => c,
                            None => continue,
                        };

                        for link in links {
                            if !link.get_constraint().matches(&candidate_constraint) {
                                req_results.push(CheckResult {
                                    platform_package: if candidate.get_name() == *require {
                                        candidate.get_pretty_name()
                                    } else {
                                        require.clone()
                                    },
                                    version: candidate_constraint.get_pretty_string(),
                                    link: Some(link.clone()),
                                    status: BString::from("<error>failed</error>"),
                                    provider: if candidate.get_name() == *require {
                                        BString::default()
                                    } else {
                                        bconcat!(
                                            "<comment>provided by ",
                                            candidate.get_pretty_name(),
                                            "</comment>"
                                        )
                                    },
                                });
                                continue 'candidates;
                            }
                        }

                        results.push(CheckResult {
                            platform_package: if candidate.get_name() == *require {
                                candidate.get_pretty_name()
                            } else {
                                require.clone()
                            },
                            version: candidate_constraint.get_pretty_string(),
                            link: None,
                            status: BString::from("<info>success</info>"),
                            provider: if candidate.get_name() == *require {
                                BString::default()
                            } else {
                                bconcat!(
                                    "<comment>provided by ",
                                    candidate.get_pretty_name(),
                                    "</comment>"
                                )
                            },
                        });
                        continue 'requirements;
                    }

                    results.extend(req_results);
                    exit_code = exit_code.max(1);
                    continue;
                }

                results.push(CheckResult {
                    platform_package: require.clone(),
                    version: BString::from("n/a"),
                    link: links.first().cloned(),
                    status: BString::from("<error>missing</error>"),
                    provider: BString::default(),
                });
                exit_code = exit_code.max(2);
            }
        }

        let format = BString::from(
            input
                .borrow()
                .get_option("format")?
                .as_string()
                .unwrap_or(BStr::new("text")),
        );
        self.print_table(_output, &results, BStr::new(&format))?;

        Ok(exit_code)
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for CheckPlatformReqsCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
