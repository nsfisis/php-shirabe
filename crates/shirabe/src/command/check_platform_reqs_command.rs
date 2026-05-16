//! ref: composer/src/Composer/Command/CheckPlatformReqsCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{PhpMixed, strip_tags};
use shirabe_semver::constraint::constraint::Constraint;

use crate::command::base_command::BaseCommand;
use crate::console::input::input_option::InputOption;
use crate::json::json_file::JsonFile;
use crate::package::link::Link;
use crate::repository::installed_repository::InstalledRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::root_package_repository::RootPackageRepository;

struct CheckResult {
    platform_package: String,
    version: String,
    link: Option<Link>,
    status: String,
    provider: String,
}

#[derive(Debug)]
pub struct CheckPlatformReqsCommand {
    inner: BaseCommand,
}

impl CheckPlatformReqsCommand {
    pub fn configure(&mut self) {
        self.inner
            .set_name("check-platform-reqs")
            .set_description("Check that platform requirements are satisfied")
            .set_definition(vec![
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables checking of require-dev packages requirements.", None, vec![]),
                InputOption::new("lock", None, Some(InputOption::VALUE_NONE), "Checks requirements only from the lock file, not from installed packages.", None, vec![]),
                InputOption::new("format", Some(shirabe_php_shim::PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Format of the output: text or json", Some(shirabe_php_shim::PhpMixed::String("text".to_string())), vec!["json".to_string(), "text".to_string()]),
            ])
            .set_help(
                "Checks that your PHP and extensions versions match the platform requirements of the installed packages.\n\n\
                Unlike update/install, this command will ignore config.platform settings and check the real platform packages so you can be certain you have the required platform dependencies.\n\n\
                <info>php composer.phar check-platform-reqs</info>\n\n"
            );
    }

    pub fn execute(
        &self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let composer = self.inner.require_composer()?;
        let io = self.inner.get_io();

        let no_dev = input.get_option("no-dev").as_bool().unwrap_or(false);

        let mut requires: IndexMap<String, Vec<Link>> = IndexMap::new();
        let mut remove_packages: Vec<String> = vec![];

        let installed_repo_base = if input.get_option("lock").as_bool().unwrap_or(false) {
            io.write_error(&format!(
                "<info>Checking {}platform requirements using the lock file</info>",
                if no_dev { "non-dev " } else { "" }
            ));
            composer.get_locker().get_locked_repository(!no_dev)?
        } else {
            let local_repo = composer.get_repository_manager().get_local_repository();
            if local_repo.get_packages().is_empty() {
                io.write_error(&format!(
                    "<warning>No vendor dir present, checking {}platform requirements from the lock file</warning>",
                    if no_dev { "non-dev " } else { "" }
                ));
                composer.get_locker().get_locked_repository(!no_dev)?
            } else {
                if no_dev {
                    remove_packages = local_repo.get_dev_package_names().clone();
                }
                io.write_error(&format!(
                    "<info>Checking {}platform requirements for packages in the vendor dir</info>",
                    if no_dev { "non-dev " } else { "" }
                ));
                local_repo.clone_box()
            }
        };

        if !no_dev {
            for (require, link) in composer.get_package().get_dev_requires() {
                requires
                    .entry(require.to_string())
                    .or_insert_with(Vec::new)
                    .push(link.clone());
            }
        }

        let root_pkg_repo = RootPackageRepository::new(composer.get_package().clone_box());
        let installed_repo =
            InstalledRepository::new(vec![installed_repo_base, Box::new(root_pkg_repo)]);

        for package in installed_repo.get_packages() {
            if remove_packages.contains(&package.get_name().to_string()) {
                continue;
            }
            for (require, link) in package.get_requires() {
                requires
                    .entry(require.to_string())
                    .or_insert_with(Vec::new)
                    .push(link.clone());
            }
        }

        let mut requires_sorted: Vec<(String, Vec<Link>)> = requires.into_iter().collect();
        requires_sorted.sort_by(|a, b| a.0.cmp(&b.0));

        let installed_repo_with_platform = InstalledRepository::new(vec![
            Box::new(installed_repo),
            Box::new(PlatformRepository::new(vec![], vec![])),
        ]);

        let mut results: Vec<CheckResult> = vec![];
        let mut exit_code = 0;

        'requirements: for (require, links) in &requires_sorted {
            if PlatformRepository::is_platform_package(require) {
                let candidates = installed_repo_with_platform
                    .find_packages_with_replacers_and_providers(require);
                if !candidates.is_empty() {
                    let mut req_results: Vec<CheckResult> = vec![];
                    'candidates: for candidate in &candidates {
                        let candidate_constraint = if candidate.get_name() == require {
                            let mut c = Constraint::new("=", candidate.get_version());
                            c.set_pretty_string(candidate.get_pretty_version());
                            Some(c)
                        } else {
                            let mut found = None;
                            for link in candidate
                                .get_provides()
                                .iter()
                                .chain(candidate.get_replaces().iter())
                            {
                                if link.get_target() == require {
                                    found = Some(link.get_constraint().clone_box());
                                    break;
                                }
                            }
                            found.map(|c| Constraint::from_constraint_interface(c))
                        };

                        let candidate_constraint = match candidate_constraint {
                            Some(c) => c,
                            None => continue,
                        };

                        for link in links {
                            if !link.get_constraint().matches(&candidate_constraint) {
                                req_results.push(CheckResult {
                                    platform_package: if candidate.get_name() == require {
                                        candidate.get_pretty_name().to_string()
                                    } else {
                                        require.clone()
                                    },
                                    version: candidate_constraint.get_pretty_string().to_string(),
                                    link: Some(link.clone()),
                                    status: "<error>failed</error>".to_string(),
                                    provider: if candidate.get_name() == require {
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
                            platform_package: if candidate.get_name() == require {
                                candidate.get_pretty_name().to_string()
                            } else {
                                require.clone()
                            },
                            version: candidate_constraint.get_pretty_string().to_string(),
                            link: None,
                            status: "<info>success</info>".to_string(),
                            provider: if candidate.get_name() == require {
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
            .get_option("format")
            .as_string()
            .unwrap_or("text")
            .to_string();
        self.print_table(_output, &results, &format);

        Ok(exit_code)
    }

    fn print_table(&self, output: &dyn OutputInterface, results: &[CheckResult], format: &str) {
        let io = self.inner.get_io();

        if format == "json" {
            let rows: Vec<PhpMixed> = results
                .iter()
                .map(|result| {
                    let mut row = IndexMap::new();
                    row.insert(
                        "name".to_string(),
                        Box::new(PhpMixed::String(result.platform_package.clone())),
                    );
                    row.insert(
                        "version".to_string(),
                        Box::new(PhpMixed::String(result.version.clone())),
                    );
                    row.insert(
                        "status".to_string(),
                        Box::new(PhpMixed::String(strip_tags(&result.status))),
                    );
                    if let Some(link) = &result.link {
                        let mut failed_req = IndexMap::new();
                        failed_req.insert(
                            "source".to_string(),
                            Box::new(PhpMixed::String(link.get_source().to_string())),
                        );
                        failed_req.insert(
                            "type".to_string(),
                            Box::new(PhpMixed::String(link.get_description().to_string())),
                        );
                        failed_req.insert(
                            "target".to_string(),
                            Box::new(PhpMixed::String(link.get_target().to_string())),
                        );
                        failed_req.insert(
                            "constraint".to_string(),
                            Box::new(PhpMixed::String(
                                link.get_pretty_constraint().unwrap_or("").to_string(),
                            )),
                        );
                        row.insert(
                            "failed_requirement".to_string(),
                            Box::new(PhpMixed::Array(failed_req)),
                        );
                    } else {
                        row.insert("failed_requirement".to_string(), Box::new(PhpMixed::Null));
                    }
                    let provider_str = strip_tags(&result.provider);
                    row.insert(
                        "provider".to_string(),
                        Box::new(if provider_str.is_empty() {
                            PhpMixed::Null
                        } else {
                            PhpMixed::String(provider_str)
                        }),
                    );
                    PhpMixed::Array(row)
                })
                .collect();

            io.write(&JsonFile::encode(&PhpMixed::List(
                rows.into_iter().map(Box::new).collect(),
            )));
        } else {
            let rows: Vec<Vec<PhpMixed>> = results
                .iter()
                .map(|result| {
                    vec![
                        PhpMixed::String(result.platform_package.clone()),
                        PhpMixed::String(result.version.clone()),
                        if let Some(link) = &result.link {
                            PhpMixed::String(format!(
                                "{} {} {} ({})",
                                link.get_source(),
                                link.get_description(),
                                link.get_target(),
                                link.get_pretty_constraint().unwrap_or(""),
                            ))
                        } else {
                            PhpMixed::String(String::new())
                        },
                        PhpMixed::String(
                            format!("{} {}", result.status, result.provider)
                                .trim_end()
                                .to_string(),
                        ),
                    ]
                })
                .collect();

            self.inner.render_table(rows, output);
        }
    }
}
