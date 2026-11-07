//! ref: composer/src/Composer/Command/LicensesCommand.php sha1:daf0ea7245bbc82d223618fb711d1e17468c3706

use crate::command::{
    BaseCommand, BaseCommandData, base_command_complete, base_command_initialize,
};
use crate::composer::composer_full;
use crate::console::input::{InputOption, SuggestedValues};
use crate::delegate_base_command_trait_impls_to_inner;
use crate::io::IOInterfaceWrite;
use crate::json::JsonFile;
use crate::package::{DisplayMode, PackageInterfaceHandle};
use crate::plugin::{CommandEvent, PluginEvents};
use crate::repository::{RepositoryInterface, RepositoryUtils};
use crate::util::{PackageInfo, PackageSorter};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{
    RuntimeException, UnexpectedValueException, bformat, impl_php_class, implode, json_bytes,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::formatter::OutputFormatter;
use shirabe_symfony_console::helper::{Cell, Row, Table};
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName, InputValue};
use shirabe_symfony_console::output::OutputInterface;
use shirabe_symfony_console::style::{StyleInterface, SymfonyStyle};

/// Shape of the `licenses --format=json` output.
#[derive(Debug, serde::Serialize)]
struct LicensesJson {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    #[serde(serialize_with = "json_bytes::str")]
    version: BString,
    #[serde(serialize_with = "json_bytes::str_vec")]
    license: Vec<BString>,
    #[serde(serialize_with = "json_bytes::keyed")]
    dependencies: IndexMap<BString, DependencyLicenseJson>,
}

#[derive(Debug, serde::Serialize)]
struct DependencyLicenseJson {
    #[serde(serialize_with = "json_bytes::str")]
    version: BString,
    #[serde(serialize_with = "json_bytes::str_vec")]
    license: Vec<BString>,
}

#[derive(Debug)]
pub struct LicensesCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(LicensesCommand, r"Composer\Command\LicensesCommand");

impl Default for LicensesCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl LicensesCommand {
    pub fn new() -> Self {
        let command = LicensesCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("LicensesCommand::configure uses static, valid metadata");
        command
    }
}

#[async_trait::async_trait(?Send)]
impl Command for LicensesCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("licenses".into())?;
        self.set_description("Shows information about licenses of dependencies".into());
        self.set_definition(&[
            InputOption::with_suggested_values(
                "format",
                Some("f".into()),
                Some(InputOption::VALUE_REQUIRED),
                "Format of the output: text, json or summary",
                Some(InputValue::from("text")),
                SuggestedValues::List(vec![
                    BString::from("text"),
                    BString::from("json"),
                    BString::from("summary"),
                ]),
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
        ]);
        self.set_help(
            "The license command displays detailed information about the licenses of\n\
            the installed dependencies.\n\n\
            Use --locked to show licenses from composer.lock instead of what's currently\n\
            installed in the vendor directory.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#licenses"
                .into(),
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer_handle = self.require_composer(None, None)?;

        let command_event = CommandEvent::new(
            PluginEvents::COMMAND,
            "licenses",
            input.clone(),
            output.clone(),
        );
        let command_event_name = command_event.get_name().to_owned();
        let event_dispatcher = composer_handle.borrow_partial().get_event_dispatcher();
        event_dispatcher.borrow_mut().dispatch(
            Some(command_event_name.as_ref()),
            Some(std::rc::Rc::new(std::cell::RefCell::new(command_event))),
        )?;

        let composer = composer_full(&composer_handle);
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
                return Err(UnexpectedValueException::new(BString::from("Valid composer.json and composer.lock files are required to run this command with --locked")).into());
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

        let packages: Vec<PackageInterfaceHandle> = packages.into_iter().collect();
        let packages = PackageSorter::sort_packages_alphabetically(packages);
        let io = self.get_io();

        let format = BString::from(
            input
                .borrow()
                .get_option("format")?
                .as_string()
                .unwrap_or(bstr::BStr::new("text")),
        );
        match format.as_slice() {
            b"text" => {
                let root_licenses = root.get_license();
                let licenses_str = if root_licenses.is_empty() {
                    BString::from("none")
                } else {
                    implode(", ", &root_licenses)
                };
                io.write(bformat!(
                    "Name: <comment>{}</comment>",
                    root.get_pretty_name()
                ));
                io.write(bformat!(
                    "Version: <comment>{}</comment>",
                    root.get_full_pretty_version(true, DisplayMode::SourceRefIfDev)
                ));
                io.write(bformat!("Licenses: <comment>{licenses_str}</comment>"));
                io.write("Dependencies:");
                io.write("");

                let mut table = Table::new(output);
                table.set_style("compact".into())??;
                table.set_headers(vec!["Name".into(), "Version".into(), "Licenses".into()]);
                for package in &packages {
                    let link = PackageInfo::get_view_source_or_homepage_url(package.clone());
                    let name = if let Some(link) = link {
                        bformat!(
                            "<href={}>{}</>",
                            OutputFormatter::escape(&link)?,
                            package.get_pretty_name()
                        )
                    } else {
                        package.get_pretty_name()
                    };
                    let pkg_licenses = if let Some(complete_pkg) = package.as_complete_package() {
                        complete_pkg.get_license()
                    } else {
                        vec![]
                    };
                    let licenses_str = if pkg_licenses.is_empty() {
                        BString::from("none")
                    } else {
                        implode(", ", &pkg_licenses)
                    };
                    table.add_row(
                        vec![
                            Cell::from(name),
                            Cell::from(
                                package.get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
                            ),
                            Cell::from(licenses_str),
                        ]
                        .into(),
                    );
                }
                table.render()?;
            }
            b"json" => {
                let dependencies: IndexMap<BString, DependencyLicenseJson> = packages
                    .iter()
                    .map(|package| {
                        let license = if let Some(complete_pkg) = package.as_complete_package() {
                            complete_pkg.get_license()
                        } else {
                            vec![]
                        };
                        (
                            package.get_pretty_name(),
                            DependencyLicenseJson {
                                version: package
                                    .get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
                                license,
                            },
                        )
                    })
                    .collect();

                io.write(&JsonFile::encode(&LicensesJson {
                    name: root.get_pretty_name(),
                    version: root.get_full_pretty_version(true, DisplayMode::SourceRefIfDev),
                    license: root.get_license(),
                    dependencies,
                })?);
            }
            b"summary" => {
                let mut used_licenses: IndexMap<BString, i64> = IndexMap::new();
                for package in &packages {
                    let mut licenses = if let Some(complete_pkg) = package.as_complete_package() {
                        complete_pkg.get_license()
                    } else {
                        vec![]
                    };
                    if licenses.is_empty() {
                        licenses.push(BString::from("none"));
                    }
                    for license_name in licenses {
                        *used_licenses.entry(license_name).or_insert(0) += 1;
                    }
                }

                let mut entries: Vec<(BString, i64)> = used_licenses.into_iter().collect();
                entries.sort_by(|a, b| b.1.cmp(&a.1));

                let rows: Vec<Row> = entries
                    .iter()
                    .map(|(license, count)| {
                        Row::Cells(vec![license.clone().into(), bformat!("{count}").into()])
                    })
                    .collect();

                let symfony_io = SymfonyStyle::new(input, output);
                symfony_io.borrow().table(
                    vec!["License".into(), "Number of dependencies".into()],
                    rows,
                )?;
            }
            _ => {
                return Err(RuntimeException::new(bformat!(
                    "Unsupported format \"{format}\".  See help for supported formats."
                ))
                .into());
            }
        }

        Ok(0)
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
        base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for LicensesCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    delegate_base_command_trait_impls_to_inner!(base_command_data);
}
