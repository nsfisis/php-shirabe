//! ref: composer/src/Composer/Command/CheckPlatformReqsCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::console::input::InputOption;
use crate::io::IOInterfaceImmutable;
use crate::json::JsonFile;
use crate::package::Link;
use crate::repository::InstalledRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RootPackageRepository;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{PhpMixed, array_merge_map, strip_tags};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;

struct CheckResult {
    platform_package: String,
    version: String,
    link: Option<Link>,
    status: String,
    provider: String,
}

#[derive(Debug)]
pub struct CheckPlatformReqsCommand {
    base_command_data: BaseCommandData,
}

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
        format: &str,
    ) {
        let io = self.get_io();

        if format == "json" {
            let rows: Vec<PhpMixed> = results
                .iter()
                .map(|result| {
                    let mut row = IndexMap::new();
                    row.insert(
                        "name".to_string(),
                        PhpMixed::String(result.platform_package.clone()),
                    );
                    row.insert(
                        "version".to_string(),
                        PhpMixed::String(result.version.clone()),
                    );
                    row.insert(
                        "status".to_string(),
                        PhpMixed::String(strip_tags(&result.status)),
                    );
                    if let Some(link) = &result.link {
                        let mut failed_req = IndexMap::new();
                        failed_req.insert(
                            "source".to_string(),
                            PhpMixed::String(link.get_source().to_string()),
                        );
                        failed_req.insert(
                            "type".to_string(),
                            PhpMixed::String(link.get_description().to_string()),
                        );
                        failed_req.insert(
                            "target".to_string(),
                            PhpMixed::String(link.get_target().to_string()),
                        );
                        failed_req.insert(
                            "constraint".to_string(),
                            PhpMixed::String(link.get_pretty_constraint().to_string()),
                        );
                        row.insert(
                            "failed_requirement".to_string(),
                            PhpMixed::Array(failed_req),
                        );
                    } else {
                        row.insert("failed_requirement".to_string(), PhpMixed::Null);
                    }
                    let provider_str = strip_tags(&result.provider);
                    row.insert(
                        "provider".to_string(),
                        if provider_str.is_empty() {
                            PhpMixed::Null
                        } else {
                            PhpMixed::String(provider_str)
                        },
                    );
                    PhpMixed::Array(row)
                })
                .collect();

            io.write(&JsonFile::encode(&PhpMixed::List(rows)));
        } else {
            let rows: Vec<PhpMixed> = results
                .iter()
                .map(|result| {
                    PhpMixed::List(vec![
                        PhpMixed::String(result.platform_package.clone()),
                        PhpMixed::String(result.version.clone()),
                        match &result.link {
                            Some(link) => PhpMixed::String(link.to_string()),
                            None => PhpMixed::String(String::new()),
                        },
                        if let Some(link) = &result.link {
                            PhpMixed::String(format!(
                                "{} {} {} ({})",
                                link.get_source(),
                                link.get_description(),
                                link.get_target(),
                                link.get_pretty_constraint(),
                            ))
                        } else {
                            PhpMixed::String(String::new())
                        },
                        PhpMixed::String(
                            format!("{} {}", result.status, result.provider)
                                .trim_end()
                                .to_string(),
                        ),
                    ])
                })
                .collect();

            self.render_table(rows, output);
        }
    }
}

impl Command for CheckPlatformReqsCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("check-platform-reqs")?;
        self.set_description("Check that platform requirements are satisfied");
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
            InputOption::new(
                "format",
                Some(shirabe_php_shim::PhpMixed::String("f".to_string())),
                Some(InputOption::VALUE_REQUIRED),
                "Format of the output: text or json",
                Some(shirabe_php_shim::PhpMixed::String("text".to_string())),
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "Checks that your PHP and extensions versions match the platform requirements of the installed packages.\n\n\
            Unlike update/install, this command will ignore config.platform settings and check the real platform packages so you can be certain you have the required platform dependencies.\n\n\
            <info>shirabe check-platform-reqs</info>\n\n"
        );
        Ok(())
    }

    fn execute(
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

        let mut requires: IndexMap<String, Vec<Link>> = IndexMap::new();
        let mut remove_packages: Vec<String> = vec![];

        let installed_repo_base: crate::repository::RepositoryInterfaceHandle = if input
            .borrow()
            .get_option("lock")?
            .as_bool()
            .unwrap_or(false)
        {
            io.write_error(&format!(
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
                io.write_error(&format!(
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
                io.write_error(&format!(
                    "<info>Checking {}platform requirements for packages in the vendor dir</info>",
                    if no_dev { "non-dev " } else { "" }
                ));
                local_repo.clone()
            }
        };

        if !no_dev {
            for (require, link) in composer.get_package().get_dev_requires() {
                requires
                    .entry(require.to_string())
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
            if remove_packages.contains(&package.get_name().to_string()) {
                continue;
            }
            for (require, link) in package.get_requires() {
                requires
                    .entry(require.to_string())
                    .or_default()
                    .push(link.clone());
            }
        }

        let mut requires_sorted: Vec<(String, Vec<Link>)> = requires.into_iter().collect();
        requires_sorted.sort_by(|a, b| a.0.cmp(&b.0));

        installed_repo.add_repository(crate::repository::RepositoryInterfaceHandle::new(
            PlatformRepository::new(vec![], indexmap::IndexMap::new())?,
        ));
        let installed_repo_with_platform = installed_repo;

        let mut results: Vec<CheckResult> = vec![];
        let mut exit_code = 0;

        'requirements: for (require, links) in &requires_sorted {
            if PlatformRepository::is_platform_package(require) {
                let candidates = installed_repo_with_platform
                    .find_packages_with_replacers_and_providers(require, None)?;
                if !candidates.is_empty() {
                    let mut req_results: Vec<CheckResult> = vec![];
                    'candidates: for candidate in &candidates {
                        let candidate_constraint: Option<AnyConstraint> = if candidate.get_name()
                            == *require
                        {
                            let c = SimpleConstraint::new(
                                "=".to_string(),
                                candidate.get_version().to_string(),
                                Some(candidate.get_pretty_version().to_string()),
                            );
                            Some(c.into())
                        } else {
                            let mut found: Option<AnyConstraint> = None;
                            let provides_and_replaces =
                                array_merge_map(candidate.get_provides(), candidate.get_replaces());
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
                                        candidate.get_pretty_name().to_string()
                                    } else {
                                        require.clone()
                                    },
                                    version: candidate_constraint.get_pretty_string().to_string(),
                                    link: Some(link.clone()),
                                    status: "<error>failed</error>".to_string(),
                                    provider: if candidate.get_name() == *require {
                                        String::new()
                                    } else {
                                        format!(
                                            "<comment>provided by {}</comment>",
                                            candidate.get_pretty_name()
                                        )
                                    },
                                });
                                continue 'candidates;
                            }
                        }

                        results.push(CheckResult {
                            platform_package: if candidate.get_name() == *require {
                                candidate.get_pretty_name().to_string()
                            } else {
                                require.clone()
                            },
                            version: candidate_constraint.get_pretty_string().to_string(),
                            link: None,
                            status: "<info>success</info>".to_string(),
                            provider: if candidate.get_name() == *require {
                                String::new()
                            } else {
                                format!(
                                    "<comment>provided by {}</comment>",
                                    candidate.get_pretty_name()
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
                    version: "n/a".to_string(),
                    link: links.first().cloned(),
                    status: "<error>missing</error>".to_string(),
                    provider: String::new(),
                });
                exit_code = exit_code.max(2);
            }
        }

        let format = input
            .borrow()
            .get_option("format")?
            .as_string()
            .unwrap_or("text")
            .to_string();
        self.print_table(_output, &results, &format);

        Ok(exit_code)
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for CheckPlatformReqsCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
