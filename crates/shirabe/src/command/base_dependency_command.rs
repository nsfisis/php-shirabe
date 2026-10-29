//! ref: composer/src/Composer/Command/BaseDependencyCommand.php

use crate::command::BaseCommand;
use crate::io::IOInterfaceImmutable;
use crate::package::version::VersionParser;
use crate::package::{Package, RootPackage};
use crate::plugin::{CommandEvent, PluginEvents};
use crate::repository::{
    CompositeRepository, DependentsEntry, FindPackageConstraint, InstalledArrayRepository,
    InstalledRepository, NeedleInput, PlatformRepository, RepositoryFactory, RepositoryInterface,
    RootPackageRepository,
};
use crate::util::PackageInfo;
use bstr::{B, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, UnexpectedValueException, legacy_lossy,
};
use shirabe_semver::constraint::{AnyConstraint, Bound};
use shirabe_symfony_console::formatter::{OutputFormatter, OutputFormatterStyle};
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::output::OutputInterface;

pub const ARGUMENT_PACKAGE: &str = "package";
pub const ARGUMENT_CONSTRAINT: &str = "version";
pub const OPTION_RECURSIVE: &str = "recursive";
pub const OPTION_TREE: &str = "tree";

pub trait BaseDependencyCommand: BaseCommand {
    const ARGUMENT_PACKAGE: &'static str = ARGUMENT_PACKAGE;
    const ARGUMENT_CONSTRAINT: &'static str = ARGUMENT_CONSTRAINT;
    const OPTION_RECURSIVE: &'static str = OPTION_RECURSIVE;
    const OPTION_TREE: &'static str = OPTION_TREE;

    fn colors(&self) -> std::cell::Ref<'_, Vec<String>>;
    fn set_colors(&self, colors: Vec<String>);

    fn do_execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        inverted: bool,
    ) -> anyhow::Result<i64> {
        // Emit command event on startup
        let composer = self.require_composer(None, None)?;
        let mut command_event = CommandEvent::new(
            PluginEvents::COMMAND,
            &self.get_name().unwrap_or_default(),
            input.clone(),
            output.clone(),
        );
        let command_event_name = command_event.get_name().to_string();
        let event_dispatcher = composer.borrow_partial().get_event_dispatcher();
        event_dispatcher
            .borrow_mut()
            .dispatch(Some(&command_event_name), Some(&mut command_event))?;
        let composer = crate::composer::composer_full(&composer);

        let mut repos: Vec<crate::repository::RepositoryInterfaceHandle> =
            vec![crate::repository::RepositoryInterfaceHandle::new(
                RootPackageRepository::new(crate::package::RootPackageInterfaceHandle::dup(
                    composer.get_package(),
                )),
            )];

        if input
            .borrow()
            .get_option("locked")?
            .as_bool()
            .unwrap_or(false)
        {
            let locker = composer.get_locker().clone();
            let mut locker = locker.borrow_mut();

            if !locker.is_locked() {
                return Err(UnexpectedValueException::new(
                    "A valid composer.lock file is required to run this command with --locked"
                        .to_string(),
                )
                .into());
            }

            repos.push(locker.get_locked_repository(true)?.into());
            let platform_overrides: IndexMap<BString, PhpMixed> = locker
                .get_platform_overrides()?
                .into_iter()
                .map(|(k, v)| (k, PhpMixed::string(v)))
                .collect();
            repos.push(crate::repository::RepositoryInterfaceHandle::new(
                PlatformRepository::new(vec![], platform_overrides, None, None)?,
            ));
        } else {
            let repository_manager = composer.get_repository_manager().clone();
            let repository_manager = repository_manager.borrow();
            let local_repo = repository_manager.get_local_repository();
            let root_pkg = composer.get_package();

            if local_repo.get_packages()?.is_empty()
                && (!root_pkg.get_requires().is_empty() || !root_pkg.get_dev_requires().is_empty())
            {
                output.borrow().writeln(
                    &[bstr::BString::from("<warning>No dependencies installed. Try running shirabe install or update, or use --locked.</warning>".to_string())],
                    shirabe_symfony_console::output::OUTPUT_NORMAL,
                );

                return Ok(1);
            }

            repos.push(local_repo);

            let platform_overrides: IndexMap<BString, PhpMixed> = composer
                .get_config()
                .borrow()
                .get("platform")
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            repos.push(crate::repository::RepositoryInterfaceHandle::new(
                PlatformRepository::new(vec![], platform_overrides, None, None)?,
            ));
        }

        let mut installed_repo = InstalledRepository::new(repos);

        let needle = input
            .borrow()
            .get_argument(Self::ARGUMENT_PACKAGE)?
            .as_string()
            .unwrap_or_default()
            .to_string();
        let text_constraint: String = if input.borrow().has_argument(Self::ARGUMENT_CONSTRAINT) {
            input
                .borrow()
                .get_argument(Self::ARGUMENT_CONSTRAINT)?
                .as_string()
                .unwrap_or("*")
                .to_string()
        } else {
            "*".to_string()
        };

        let packages = installed_repo
            .find_packages_with_replacers_and_providers(bstr::BStr::new(&needle), None)?;
        if packages.is_empty() {
            return Err(InvalidArgumentException::new(format!(
                "Could not find package \"{}\" in your project",
                needle
            ))
            .into());
        }

        let matched_package = installed_repo.find_package(
            bstr::BStr::new(&needle),
            FindPackageConstraint::String(bstr::BString::from(text_constraint.clone())),
        )?;
        match &matched_package {
            None => {
                let rm = composer.get_repository_manager();
                let mut default_repos = CompositeRepository::new(
                    RepositoryFactory::default_repos(
                        Some(self.get_io()),
                        Some(composer.get_config()),
                        Some(&mut *rm.borrow_mut()),
                    )?
                    .into_values()
                    .collect(),
                );
                if let Some(r#match) = default_repos.find_package(
                    bstr::BStr::new(&needle),
                    FindPackageConstraint::String(bstr::BString::from(text_constraint.clone())),
                )? {
                    installed_repo.add_repository(
                        crate::repository::RepositoryInterfaceHandle::new(
                            InstalledArrayRepository::new_with_packages(vec![
                                crate::package::PackageInterfaceHandle::dup(&r#match),
                            ])?,
                        ),
                    );
                } else if PlatformRepository::is_platform_package(&needle) {
                    let parser = VersionParser::new();
                    let platform_constraint = parser.parse_constraints(&text_constraint)?;
                    if platform_constraint.get_lower_bound() != Bound::zero() {
                        let version =
                            legacy_lossy(platform_constraint.get_lower_bound().get_version())
                                .into_owned();
                        let temp_platform_pkg = Package::new(
                            bstr::BString::from(needle.clone()),
                            bstr::BString::from(version.clone()),
                            bstr::BString::from(version),
                        );
                        installed_repo.add_repository(
                            crate::repository::RepositoryInterfaceHandle::new(
                                InstalledArrayRepository::new_with_packages(vec![
                                    crate::package::PackageHandle::from_package(temp_platform_pkg)
                                        .into(),
                                ])?,
                            ),
                        );
                    }
                } else {
                    self.get_io().write_error(&format!(
                        "<error>Package \"{}\" could not be found with constraint \"{}\", results below will most likely be incomplete.</error>",
                        needle, text_constraint
                    ));
                }
            }
            Some(matched) if PlatformRepository::is_platform_package(&needle) => {
                let extra_notice = if matched
                    .get_extra()
                    .get(B("config.platform"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    " (version provided by config.platform)"
                } else {
                    ""
                };
                self.get_io().write_error(&format!(
                    "<info>Package \"{} {}\" found in version \"{}\"{}.</info>",
                    needle,
                    text_constraint,
                    matched.get_pretty_version().display_lossy(),
                    extra_notice
                ));
            }
            Some(matched) if inverted => {
                self.get_io().write(&format!(
                    "<comment>Package \"{}\" {} is already installed! To find out why, run `shirabe why {}`</comment>",
                    needle,
                    legacy_lossy(&matched.get_pretty_version()),
                    needle
                ));

                return Ok(0);
            }
            Some(_) => {}
        }

        let mut needles = vec![needle.clone()];
        if inverted {
            for package in &packages {
                let replaces: Vec<String> = package
                    .get_replaces()
                    .values()
                    .map(|link| legacy_lossy(link.get_target()).into_owned())
                    .collect();
                needles.extend(replaces);
            }
        }

        let has_constraint = text_constraint != "*";
        let constraint: Option<AnyConstraint> = if has_constraint {
            let version_parser = VersionParser::new();
            Some(version_parser.parse_constraints(&text_constraint)?)
        } else {
            None
        };

        let render_tree = input
            .borrow()
            .get_option(Self::OPTION_TREE)?
            .as_bool()
            .unwrap_or(false);
        let recursive = render_tree
            || input
                .borrow()
                .get_option(Self::OPTION_RECURSIVE)?
                .as_bool()
                .unwrap_or(false);

        let mut r#return: i64 = if inverted { 1 } else { 0 };

        let results = installed_repo.get_dependents(
            NeedleInput::Multiple(
                needles
                    .into_iter()
                    .map(bstr::BString::from)
                    .collect::<Vec<_>>(),
            ),
            constraint,
            inverted,
            recursive,
            None,
        )?;
        if results.is_empty() {
            let extra = if has_constraint {
                format!(
                    " in versions {}matching {}",
                    if inverted { "not " } else { "" },
                    text_constraint
                )
            } else {
                String::new()
            };
            self.get_io().write_error(&format!(
                "<info>There is no installed package depending on \"{}\"{}",
                needle, extra
            ));
            r#return = if inverted { 0 } else { 1 };
        } else if render_tree {
            self.init_styles(output);
            let root = &packages[0];
            let description = root
                .as_complete()
                .and_then(|c| c.get_description())
                .unwrap_or_default();
            self.get_io().write(&format!(
                "<info>{}</info> {} {}",
                legacy_lossy(&root.get_pretty_name()),
                legacy_lossy(&root.get_pretty_version()),
                legacy_lossy(&description)
            ));
            self.print_tree(&results, "", 1);
        } else {
            self.print_table(output, results);
        }

        if inverted
            && input.borrow().has_argument(Self::ARGUMENT_CONSTRAINT)
            && !PlatformRepository::is_platform_package(&needle)
        {
            let mut composer_command = "update";

            for root_requirement in composer.get_package().get_requires().values() {
                if root_requirement.get_target() == needle.as_str() {
                    composer_command = "require";
                    break;
                }
            }

            for root_requirement in composer.get_package().get_dev_requires().values() {
                if root_requirement.get_target() == needle.as_str() {
                    composer_command = "require --dev";
                    break;
                }
            }

            self.get_io().write_error(&format!(
                "Not finding what you were looking for? Try calling `shirabe {} \"{}:{}\" --dry-run` to get another view on the problem.",
                composer_command, needle, text_constraint
            ));
        }

        Ok(r#return)
    }

    fn print_table(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        results: Vec<DependentsEntry>,
    ) {
        let mut table: Vec<Vec<String>> = vec![];
        let mut doubles: IndexMap<String, bool> = IndexMap::new();
        let mut results = results;
        loop {
            if results.is_empty() {
                break;
            }
            let mut queue: Vec<DependentsEntry> = vec![];
            let mut rows: Vec<Vec<String>> = vec![];
            for DependentsEntry(package, link, children) in results {
                let unique = legacy_lossy(&link.to_php_string()).into_owned();
                if doubles.contains_key(&unique) {
                    continue;
                }
                doubles.insert(unique.clone(), true);
                let version = if package.get_pretty_version() == RootPackage::DEFAULT_PRETTY_VERSION
                {
                    "-".to_string()
                } else {
                    legacy_lossy(&package.get_pretty_version()).into_owned()
                };
                let package_url = PackageInfo::get_view_source_or_homepage_url(package.clone());
                let name_with_link = match &package_url {
                    Some(url) => format!(
                        "<href={}>{}</>",
                        legacy_lossy(
                            &OutputFormatter::escape(url)
                                .expect("OutputFormatter::escape never fails")
                        ),
                        legacy_lossy(&package.get_pretty_name())
                    ),
                    None => legacy_lossy(&package.get_pretty_name()).into_owned(),
                };
                rows.push(vec![
                    name_with_link,
                    version,
                    legacy_lossy(link.get_description()).into_owned(),
                    format!(
                        "{} ({})",
                        legacy_lossy(&link.get_target()),
                        legacy_lossy(&link.get_pretty_constraint())
                    ),
                ]);
                if let Some(children_vec) = children {
                    queue.extend(children_vec);
                }
            }
            results = queue;
            let mut new_table = rows;
            new_table.extend(table);
            table = new_table;
        }
        let table_as_mixed: Vec<PhpMixed> = table
            .into_iter()
            .map(|row| PhpMixed::List(row.into_iter().map(PhpMixed::string).collect()))
            .collect();
        self.render_table(table_as_mixed, output);
    }

    fn init_styles(&self, output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) {
        self.set_colors(vec![
            "green".to_string(),
            "yellow".to_string(),
            "cyan".to_string(),
            "magenta".to_string(),
            "blue".to_string(),
        ]);
        for color in self.colors().iter() {
            let style = OutputFormatterStyle::new(Some(bstr::BStr::new(color)), None, vec![]);
            output
                .borrow()
                .get_formatter()
                .borrow_mut()
                .set_style(bstr::BStr::new(color), Box::new(style));
        }
    }

    fn print_tree(&self, results: &[DependentsEntry], prefix: &str, level: i64) {
        let count = results.len() as i64;
        let mut idx: i64 = 0;
        let colors = self.colors();
        let colors_len = colors.len() as i64;
        for result in results {
            let DependentsEntry(package, link, children) = result;
            let color = &colors[(level % colors_len) as usize];
            let prev_color = &colors[((level - 1) % colors_len) as usize];
            idx += 1;
            let is_last = idx == count;
            let version_text =
                if package.get_pretty_version() == RootPackage::DEFAULT_PRETTY_VERSION {
                    String::new()
                } else {
                    legacy_lossy(&package.get_pretty_version()).into_owned()
                };
            let package_url = PackageInfo::get_view_source_or_homepage_url(package.clone());
            let name_with_link = match &package_url {
                Some(url) => format!(
                    "<href={}>{}</>",
                    legacy_lossy(
                        &OutputFormatter::escape(url).expect("OutputFormatter::escape never fails")
                    ),
                    legacy_lossy(&package.get_pretty_name())
                ),
                None => legacy_lossy(&package.get_pretty_name()).into_owned(),
            };
            let package_text =
                format!("<{}>{}</{}> {}", color, name_with_link, color, version_text)
                    .trim_end()
                    .to_string();
            let link_text = format!(
                "{} <{}>{}</{}> {}",
                legacy_lossy(&link.get_description()),
                prev_color,
                legacy_lossy(&link.get_target()),
                prev_color,
                legacy_lossy(&link.get_pretty_constraint()),
            );
            let circular_warn = if children.is_none() {
                "(circular dependency aborted here)"
            } else {
                ""
            };
            self.write_tree_line(
                format!(
                    "{}{}{} ({}) {}",
                    prefix,
                    if is_last { "└──" } else { "├──" },
                    package_text,
                    link_text,
                    circular_warn
                )
                .trim_end(),
            );
            if let Some(children_vec) = children {
                self.print_tree(
                    children_vec,
                    &format!("{}{}", prefix, if is_last { "   " } else { "│  " }),
                    level + 1,
                );
            }
        }
    }

    fn write_tree_line(&self, line: &str) {
        let io = self.get_io();
        let line = if !io.is_decorated() {
            line.replace('└', "`-")
                .replace('├', "|-")
                .replace("──", "-")
                .replace('│', "|")
        } else {
            line.to_string()
        };
        io.write(&line);
    }
}
