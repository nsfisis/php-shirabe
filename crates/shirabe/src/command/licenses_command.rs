//! ref: composer/src/Composer/Command/LicensesCommand.php

use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::console::formatter::output_formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::helper::table::Table;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_external_packages::symfony::console::style::symfony_style::SymfonyStyle;
use shirabe_php_shim::{PhpMixed, RuntimeException, UnexpectedValueException};

use crate::command::base_command::BaseCommand;
use crate::composer::Composer;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::package::complete_package::CompletePackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::repository::repository_utils::RepositoryUtils;
use crate::util::package_info::PackageInfo;
use crate::util::package_sorter::PackageSorter;

#[derive(Debug)]
pub struct LicensesCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl LicensesCommand {
    pub fn configure(&mut self) {
        self.inner
            .set_name("licenses")
            .set_description("Shows information about licenses of dependencies")
            .set_definition(vec![
                InputOption::new(
                    "format",
                    Some(PhpMixed::String("f".to_string())),
                    Some(InputOption::VALUE_REQUIRED),
                    "Format of the output: text, json or summary",
                    Some(PhpMixed::String("text".to_string())),
                    vec![
                        "text".to_string(),
                        "json".to_string(),
                        "summary".to_string(),
                    ],
                ),
                InputOption::new(
                    "no-dev",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Disables search in require-dev packages.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "locked",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Shows licenses from the lock file instead of installed packages.",
                    None,
                    vec![],
                ),
            ])
            .set_help(
                "The license command displays detailed information about the licenses of\n\
                the installed dependencies.\n\n\
                Use --locked to show licenses from composer.lock instead of what's currently\n\
                installed in the vendor directory.\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#licenses",
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> Result<i64> {
        let composer = self.inner.require_composer()?;

        // TODO(plugin): dispatch COMMAND event for plugin hooks
        let command_event =
            CommandEvent::new(PluginEvents::COMMAND, "licenses".to_string(), input, output);
        composer
            .get_event_dispatcher()
            .dispatch(command_event.get_name(), &command_event);

        let root = composer.get_package();

        let packages = if input.get_option("locked").as_bool().unwrap_or(false) {
            if !composer.get_locker().is_locked() {
                return Err(UnexpectedValueException {
                    message: "Valid composer.json and composer.lock files are required to run this command with --locked".to_string(),
                    code: 0,
                }.into());
            }
            let locker = composer.get_locker();
            let no_dev = input.get_option("no-dev").as_bool().unwrap_or(false);
            let repo = locker.get_locked_repository(!no_dev)?;
            repo.get_packages()
        } else {
            let repo = composer.get_repository_manager().get_local_repository();
            if input.get_option("no-dev").as_bool().unwrap_or(false) {
                RepositoryUtils::filter_required_packages(repo.get_packages(), root)
            } else {
                repo.get_packages()
            }
        };

        let packages = PackageSorter::sort_packages_alphabetically(packages);
        let io = self.inner.get_io();

        let format = input
            .get_option("format")
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
                    root.get_full_pretty_version()
                ));
                io.write(&format!("Licenses: <comment>{}</comment>", licenses_str));
                io.write("Dependencies:");
                io.write("");

                let mut table = Table::new(output);
                table.set_style("compact");
                table.set_headers(vec![
                    "Name".to_string(),
                    "Version".to_string(),
                    "Licenses".to_string(),
                ]);
                for package in &packages {
                    let link = PackageInfo::get_view_source_or_homepage_url(package.as_ref());
                    let name = if let Some(link) = link {
                        format!(
                            "<href={}>{}</>",
                            OutputFormatter::escape(&link),
                            package.get_pretty_name()
                        )
                    } else {
                        package.get_pretty_name().to_string()
                    };
                    let pkg_licenses = if let Some(complete_pkg) =
                        (package.as_any() as &dyn Any).downcast_ref::<CompletePackage>()
                    {
                        complete_pkg.get_license()
                    } else {
                        vec![]
                    };
                    let licenses_str = if pkg_licenses.is_empty() {
                        "none".to_string()
                    } else {
                        pkg_licenses.join(", ")
                    };
                    table.add_row(vec![
                        name,
                        package.get_full_pretty_version().to_string(),
                        licenses_str,
                    ]);
                }
                table.render();
            }
            "json" => {
                let mut dependencies: IndexMap<String, IndexMap<String, PhpMixed>> =
                    IndexMap::new();
                for package in &packages {
                    let pkg_licenses = if let Some(complete_pkg) =
                        (package.as_any() as &dyn Any).downcast_ref::<CompletePackage>()
                    {
                        complete_pkg.get_license()
                    } else {
                        vec![]
                    };
                    let mut dep_info: IndexMap<String, PhpMixed> = IndexMap::new();
                    dep_info.insert(
                        "version".to_string(),
                        PhpMixed::String(package.get_full_pretty_version().to_string()),
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
                    PhpMixed::String(root.get_pretty_name().to_string()),
                );
                output_map.insert(
                    "version".to_string(),
                    PhpMixed::String(root.get_full_pretty_version().to_string()),
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
                io.write(&JsonFile::encode(&output_map));
            }
            "summary" => {
                let mut used_licenses: IndexMap<String, i64> = IndexMap::new();
                for package in &packages {
                    let mut licenses = if let Some(complete_pkg) =
                        (package.as_any() as &dyn Any).downcast_ref::<CompletePackage>()
                    {
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

                let rows: Vec<Vec<String>> = entries
                    .iter()
                    .map(|(license, count)| vec![license.clone(), count.to_string()])
                    .collect();

                let symfony_io = SymfonyStyle::new(input, output);
                symfony_io.table(
                    vec!["License".to_string(), "Number of dependencies".to_string()],
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

impl BaseCommand for LicensesCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.io
    }
}
