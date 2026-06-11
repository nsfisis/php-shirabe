//! ref: composer/src/Composer/Command/LicensesCommand.php

use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::helper::Table;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_external_packages::symfony::console::style::StyleInterface;
use shirabe_external_packages::symfony::console::style::SymfonyStyle;
use shirabe_php_shim::{PhpMixed, RuntimeException, UnexpectedValueException};

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputOption;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::json::JsonFile;
use crate::package::BasePackage;
use crate::package::CompletePackage;
use crate::package::CompletePackageInterface;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::repository::CanonicalPackagesTrait;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryUtils;
use crate::util::PackageInfo;
use crate::util::PackageSorter;

#[derive(Debug)]
pub struct LicensesCommand {
    base_command_data: BaseCommandData,
}

impl LicensesCommand {
    pub fn configure(&mut self) {
        self.set_name("licenses")
            .set_description("Shows information about licenses of dependencies")
            .set_definition(&[
                InputOption::new(
                    "format",
                    Some(PhpMixed::String("f".to_string())),
                    Some(InputOption::VALUE_REQUIRED),
                    "Format of the output: text, json or summary",
                    Some(PhpMixed::String("text".to_string())),
                )
                .unwrap()
                .into(),
                InputOption::new(
                    "no-dev",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Disables search in require-dev packages.",
                    None,
                )
                .unwrap()
                .into(),
                InputOption::new(
                    "locked",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Shows licenses from the lock file instead of installed packages.",
                    None,
                )
                .unwrap()
                .into(),
            ])
            .set_help(
                "The license command displays detailed information about the licenses of\n\
                the installed dependencies.\n\n\
                Use --locked to show licenses from composer.lock instead of what's currently\n\
                installed in the vendor directory.\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#licenses",
            );
    }

    pub fn execute(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;
        let mut composer = crate::command::composer_full_mut(&composer);

        // TODO(plugin): dispatch COMMAND event for plugin hooks
        let command_event = CommandEvent::new(
            PluginEvents::COMMAND,
            "licenses",
            input.clone(),
            output.clone(),
        );
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None);

        let root = composer.get_package();

        let packages = if input
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
            let no_dev = input
                .borrow()
                .get_option("no-dev")?
                .as_bool()
                .unwrap_or(false);
            let repo = locker.get_locked_repository(!no_dev)?;
            repo.borrow_mut().get_packages()?
        } else {
            let repository_manager = composer.get_repository_manager().clone();
            let repository_manager = repository_manager.borrow();
            let repo = repository_manager.get_local_repository();

            if input
                .borrow()
                .get_option("no-dev")?
                .as_bool()
                .unwrap_or(false)
            {
                RepositoryUtils::filter_required_packages(
                    &repo.get_packages()?,
                    composer.get_package().clone().into(),
                    false,
                    vec![],
                )
            } else {
                repo.get_packages()?
            }
        };

        let packages: Vec<crate::package::PackageInterfaceHandle> =
            packages.into_iter().map(|p| p.into()).collect();
        let packages = PackageSorter::sort_packages_alphabetically(packages);
        let io = self.get_io();

        let format = input
            .borrow()
            .get_option("format")?
            .as_string()
            .unwrap_or("text")
            .to_string();
        match format.as_str() {
            "text" => {
                let root_licenses = root.get_license();
                let licenses_str = if root_licenses.is_empty() {
                    "none".to_string()
                } else {
                    root_licenses.join(", ")
                };
                io.write(&format!(
                    "Name: <comment>{}</comment>",
                    root.get_pretty_name()
                ));
                io.write(&format!(
                    "Version: <comment>{}</comment>",
                    root.get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
                ));
                io.write(&format!("Licenses: <comment>{}</comment>", licenses_str));
                io.write("Dependencies:");
                io.write("");

                let mut table = Table::new(output);
                table.set_style(shirabe_php_shim::PhpMixed::from("compact"))??;
                table.set_headers(vec![
                    PhpMixed::String("Name".to_string()),
                    PhpMixed::String("Version".to_string()),
                    PhpMixed::String("Licenses".to_string()),
                ]);
                for package in &packages {
                    let link = PackageInfo::get_view_source_or_homepage_url(package.clone());
                    let name = if let Some(link) = link {
                        format!(
                            "<href={}>{}</>",
                            OutputFormatter::escape(&link)?,
                            package.get_pretty_name()
                        )
                    } else {
                        package.get_pretty_name().to_string()
                    };
                    let pkg_licenses = if let Some(complete_pkg) = package.as_complete_package() {
                        complete_pkg.get_license()
                    } else {
                        vec![]
                    };
                    let licenses_str = if pkg_licenses.is_empty() {
                        "none".to_string()
                    } else {
                        pkg_licenses.join(", ")
                    };
                    table.add_row(PhpMixed::List(vec![
                        Box::new(PhpMixed::String(name)),
                        Box::new(PhpMixed::String(package.get_full_pretty_version(
                            true,
                            crate::package::DisplayMode::SourceRefIfDev,
                        ))),
                        Box::new(PhpMixed::String(licenses_str)),
                    ]));
                }
                table.render();
            }
            "json" => {
                let mut dependencies: IndexMap<String, IndexMap<String, PhpMixed>> =
                    IndexMap::new();
                for package in &packages {
                    let pkg_licenses = if let Some(complete_pkg) = package.as_complete_package() {
                        complete_pkg.get_license()
                    } else {
                        vec![]
                    };
                    let mut dep_info: IndexMap<String, PhpMixed> = IndexMap::new();
                    dep_info.insert(
                        "version".to_string(),
                        PhpMixed::String(package.get_full_pretty_version(
                            true,
                            crate::package::DisplayMode::SourceRefIfDev,
                        )),
                    );
                    dep_info.insert(
                        "license".to_string(),
                        PhpMixed::List(
                            pkg_licenses
                                .into_iter()
                                .map(|l| Box::new(PhpMixed::String(l)))
                                .collect(),
                        ),
                    );
                    dependencies.insert(package.get_pretty_name().to_string(), dep_info);
                }

                let mut output_map: IndexMap<String, PhpMixed> = IndexMap::new();
                output_map.insert(
                    "name".to_string(),
                    PhpMixed::String(root.get_pretty_name().clone()),
                );
                output_map.insert(
                    "version".to_string(),
                    PhpMixed::String(
                        root.get_full_pretty_version(
                            true,
                            crate::package::DisplayMode::SourceRefIfDev,
                        )
                        .clone(),
                    ),
                );
                let root_licenses = root.get_license();
                output_map.insert(
                    "license".to_string(),
                    PhpMixed::List(
                        root_licenses
                            .into_iter()
                            .map(|l| Box::new(PhpMixed::String(l)))
                            .collect(),
                    ),
                );
                output_map.insert(
                    "dependencies".to_string(),
                    PhpMixed::Array(
                        dependencies
                            .into_iter()
                            .map(|(k, v)| {
                                (
                                    k,
                                    Box::new(PhpMixed::Array(
                                        v.into_iter().map(|(k2, v2)| (k2, Box::new(v2))).collect(),
                                    )),
                                )
                            })
                            .collect(),
                    ),
                );
                io.write(&JsonFile::encode(&PhpMixed::Array(
                    output_map
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                )));
            }
            "summary" => {
                let mut used_licenses: IndexMap<String, i64> = IndexMap::new();
                for package in &packages {
                    let mut licenses = if let Some(complete_pkg) = package.as_complete_package() {
                        complete_pkg.get_license()
                    } else {
                        vec![]
                    };
                    if licenses.is_empty() {
                        licenses.push("none".to_string());
                    }
                    for license_name in licenses {
                        *used_licenses.entry(license_name).or_insert(0) += 1;
                    }
                }

                let mut entries: Vec<(String, i64)> = used_licenses.into_iter().collect();
                entries.sort_by(|a, b| b.1.cmp(&a.1));

                let rows: Vec<PhpMixed> = entries
                    .iter()
                    .map(|(license, count)| {
                        PhpMixed::List(vec![
                            Box::new(PhpMixed::String(license.clone())),
                            Box::new(PhpMixed::String(count.to_string())),
                        ])
                    })
                    .collect();

                let mut symfony_io = SymfonyStyle::new(input, output);
                symfony_io.table(
                    vec![
                        PhpMixed::String("License".to_string()),
                        PhpMixed::String("Number of dependencies".to_string()),
                    ],
                    rows,
                );
            }
            _ => {
                return Err(RuntimeException {
                    message: format!(
                        "Unsupported format \"{}\".  See help for supported formats.",
                        format
                    ),
                    code: 0,
                }
                .into());
            }
        }

        Ok(0)
    }
}

impl HasBaseCommandData for LicensesCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
