//! ref: composer/src/Composer/Command/BaseDependencyCommand.php

use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::formatter::output_formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::formatter::output_formatter_style::OutputFormatterStyle;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{InvalidArgumentException, UnexpectedValueException};
use shirabe_semver::constraint::bound::Bound;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

use crate::command::base_command::BaseCommand;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::link::Link;
use crate::package::package::Package;
use crate::package::root_package::RootPackage;
use crate::package::version::version_parser::VersionParser;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::installed_array_repository::InstalledArrayRepository;
use crate::repository::installed_repository::{DependentsEntry, InstalledRepository, NeedleInput};
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_factory::RepositoryFactory;
use crate::repository::repository_interface::{FindPackageConstraint, RepositoryInterface};
use crate::repository::root_package_repository::RootPackageRepository;
use crate::util::package_info::PackageInfo;

#[derive(Debug)]
pub struct BaseDependencyCommand {
    inner: BaseCommand,
    pub(crate) colors: Vec<String>,
}

impl BaseDependencyCommand {
    pub const ARGUMENT_PACKAGE: &'static str = "package";
    pub const ARGUMENT_CONSTRAINT: &'static str = "version";
    pub const OPTION_RECURSIVE: &'static str = "recursive";
    pub const OPTION_TREE: &'static str = "tree";

    pub fn set_name(&mut self, name: &str) -> &mut Self {
        self.inner.set_name(name);
        self
    }

    pub fn set_aliases(&mut self, aliases: Vec<String>) -> &mut Self {
        self.inner.set_aliases(aliases);
        self
    }

    pub fn set_description(&mut self, description: &str) -> &mut Self {
        self.inner.set_description(description);
        self
    }

    pub fn set_definition(&mut self, definition: Vec<shirabe_php_shim::PhpMixed>) -> &mut Self {
        self.inner.set_definition(definition);
        self
    }

    pub fn set_help(&mut self, help: &str) -> &mut Self {
        self.inner.set_help(help);
        self
    }

    pub(crate) fn do_execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
        inverted: bool,
    ) -> anyhow::Result<i64> {
        let composer = self.inner.require_composer()?;
        // TODO(plugin): dispatch CommandEvent(PluginEvents::COMMAND, self.inner.get_name(), input, output) via composer.get_event_dispatcher()

        let mut repos: Vec<Box<dyn RepositoryInterface>> = vec![];
        repos.push(Box::new(RootPackageRepository::new(
            composer.get_package().clone_box(),
        )));

        if input.get_option("locked").as_bool().unwrap_or(false) {
            let locker = composer.get_locker();

            if !locker.is_locked() {
                return Err(anyhow::anyhow!(UnexpectedValueException {
                    message: "A valid composer.lock file is required to run this command with --locked"
                        .to_string(),
                    code: 0,
                }));
            }

            repos.push(Box::new(locker.get_locked_repository(true)?));
            repos.push(Box::new(PlatformRepository::new(
                vec![],
                locker.get_platform_overrides(),
            )));
        } else {
            let local_repo = composer.get_repository_manager().get_local_repository();
            let root_pkg = composer.get_package();

            if local_repo.get_packages().len() == 0
                && (root_pkg.get_requires().len() > 0 || root_pkg.get_dev_requires().len() > 0)
            {
                output.writeln(
                    "<warning>No dependencies installed. Try running composer install or update, or use --locked.</warning>",
                );

                return Ok(1);
            }

            repos.push(Box::new(local_repo));

            let platform_overrides = composer.get_config().get("platform").unwrap_or_default();
            repos.push(Box::new(PlatformRepository::new(vec![], platform_overrides)));
        }

        let mut installed_repo = InstalledRepository::new(repos)?;

        let needle = input
            .get_argument(Self::ARGUMENT_PACKAGE)
            .as_string()
            .unwrap_or_default()
            .to_string();
        let text_constraint: String = if input.has_argument(Self::ARGUMENT_CONSTRAINT) {
            input
                .get_argument(Self::ARGUMENT_CONSTRAINT)
                .as_string()
                .unwrap_or("*")
                .to_string()
        } else {
            "*".to_string()
        };

        let packages =
            installed_repo.find_packages_with_replacers_and_providers(needle.clone(), None);
        if packages.is_empty() {
            return Err(anyhow::anyhow!(InvalidArgumentException {
                message: format!("Could not find package \"{}\" in your project", needle),
                code: 0,
            }));
        }

        let matched_package = installed_repo.find_package(
            needle.clone(),
            FindPackageConstraint::String(text_constraint.clone()),
        );
        if matched_package.is_none() {
            let default_repos = CompositeRepository::new(RepositoryFactory::default_repos(
                Some(self.inner.get_io()),
                Some(composer.get_config()),
                Some(&mut composer.get_repository_manager()),
            )?);
            if let Some(r#match) = default_repos.find_package(
                needle.clone(),
                FindPackageConstraint::String(text_constraint.clone()),
            ) {
                installed_repo.add_repository(Box::new(InstalledArrayRepository::new(vec![
                    r#match.clone_box(),
                ])))?;
            } else if PlatformRepository::is_platform_package(&needle) {
                let parser = VersionParser::new();
                let platform_constraint = parser.parse_constraints(&text_constraint)?;
                if platform_constraint.get_lower_bound() != Bound::zero() {
                    let version = platform_constraint.get_lower_bound().get_version().to_string();
                    let temp_platform_pkg = Package::new(needle.clone(), version.clone(), version);
                    installed_repo.add_repository(Box::new(InstalledArrayRepository::new(vec![
                        Box::new(temp_platform_pkg),
                    ])))?;
                }
            } else {
                self.inner.get_io().write_error(&format!(
                    "<error>Package \"{}\" could not be found with constraint \"{}\", results below will most likely be incomplete.</error>",
                    needle, text_constraint
                ));
            }
        } else if PlatformRepository::is_platform_package(&needle) {
            let matched = matched_package.as_ref().unwrap();
            let extra_notice = if matched
                .get_extra()
                .get("config.platform")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                " (version provided by config.platform)"
            } else {
                ""
            };
            self.inner.get_io().write_error(&format!(
                "<info>Package \"{} {}\" found in version \"{}\"{}.</info>",
                needle,
                text_constraint,
                matched.get_pretty_version(),
                extra_notice
            ));
        } else if inverted {
            let matched = matched_package.as_ref().unwrap();
            self.inner.get_io().write(&format!(
                "<comment>Package \"{}\" {} is already installed! To find out why, run `composer why {}`</comment>",
                needle,
                matched.get_pretty_version(),
                needle
            ));

            return Ok(0);
        }

        let mut needles = vec![needle.clone()];
        if inverted {
            for package in &packages {
                let replaces: Vec<String> = package
                    .get_replaces()
                    .values()
                    .map(|link| link.get_target().to_string())
                    .collect();
                needles.extend(replaces);
            }
        }

        let has_constraint = text_constraint != "*";
        let constraint = if has_constraint {
            let version_parser = VersionParser::new();
            Some(version_parser.parse_constraints(&text_constraint)?)
        } else {
            None
        };

        let render_tree = input.get_option(Self::OPTION_TREE).as_bool().unwrap_or(false);
        let recursive =
            render_tree || input.get_option(Self::OPTION_RECURSIVE).as_bool().unwrap_or(false);

        let mut r#return: i64 = if inverted { 1 } else { 0 };

        let results = installed_repo.get_dependents(
            NeedleInput::Multiple(needles),
            constraint,
            inverted,
            recursive,
            None,
        );
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
            self.inner.get_io().write_error(&format!(
                "<info>There is no installed package depending on \"{}\"{}",
                needle, extra
            ));
            r#return = if inverted { 0 } else { 1 };
        } else if render_tree {
            self.init_styles(output);
            let root = &packages[0];
            let description = root
                .as_complete_package_interface()
                .and_then(|c| c.get_description())
                .unwrap_or("");
            self.inner.get_io().write(&format!(
                "<info>{}</info> {} {}",
                root.get_pretty_name(),
                root.get_pretty_version(),
                description
            ));
            self.print_tree(&results, "", 1);
        } else {
            self.print_table(output, results);
        }

        if inverted
            && input.has_argument(Self::ARGUMENT_CONSTRAINT)
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

            self.inner.get_io().write_error(&format!(
                "Not finding what you were looking for? Try calling `composer {} \"{}:{}\" --dry-run` to get another view on the problem.",
                composer_command, needle, text_constraint
            ));
        }

        Ok(r#return)
    }

    pub(crate) fn print_table(
        &self,
        output: &dyn OutputInterface,
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
                let unique = link.to_string();
                if doubles.contains_key(&unique) {
                    continue;
                }
                doubles.insert(unique.clone(), true);
                let version =
                    if package.get_pretty_version() == RootPackage::DEFAULT_PRETTY_VERSION {
                        "-".to_string()
                    } else {
                        package.get_pretty_version().to_string()
                    };
                let package_url = PackageInfo::get_view_source_or_homepage_url(&*package);
                let name_with_link = match &package_url {
                    Some(url) => format!(
                        "<href={}>{}</>",
                        OutputFormatter::escape(url),
                        package.get_pretty_name()
                    ),
                    None => package.get_pretty_name().to_string(),
                };
                rows.push(vec![
                    name_with_link,
                    version,
                    link.get_description().to_string(),
                    format!(
                        "{} ({})",
                        link.get_target(),
                        link.get_pretty_constraint().unwrap_or("")
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
        self.inner.render_table(table, output);
    }

    pub(crate) fn init_styles(&mut self, output: &dyn OutputInterface) {
        self.colors = vec![
            "green".to_string(),
            "yellow".to_string(),
            "cyan".to_string(),
            "magenta".to_string(),
            "blue".to_string(),
        ];
        for color in &self.colors {
            let style = OutputFormatterStyle::new(color.clone());
            output.get_formatter().set_style(color, style);
        }
    }

    pub(crate) fn print_tree(&self, results: &[DependentsEntry], prefix: &str, level: i64) {
        let count = results.len() as i64;
        let mut idx: i64 = 0;
        let colors_len = self.colors.len() as i64;
        for result in results {
            let DependentsEntry(package, link, children) = result;
            let color = &self.colors[(level % colors_len) as usize];
            let prev_color = &self.colors[((level - 1) % colors_len) as usize];
            idx += 1;
            let is_last = idx == count;
            let version_text =
                if package.get_pretty_version() == RootPackage::DEFAULT_PRETTY_VERSION {
                    String::new()
                } else {
                    package.get_pretty_version().to_string()
                };
            let package_url = PackageInfo::get_view_source_or_homepage_url(&**package);
            let name_with_link = match &package_url {
                Some(url) => format!(
                    "<href={}>{}</>",
                    OutputFormatter::escape(url),
                    package.get_pretty_name()
                ),
                None => package.get_pretty_name().to_string(),
            };
            let package_text =
                format!("<{}>{}</{}> {}", color, name_with_link, color, version_text)
                    .trim_end()
                    .to_string();
            let link_text = format!(
                "{} <{}>{}</{}> {}",
                link.get_description(),
                prev_color,
                link.get_target(),
                prev_color,
                link.get_pretty_constraint().unwrap_or("")
            );
            let circular_warn = if children.is_none() {
                "(circular dependency aborted here)"
            } else {
                ""
            };
            self.write_tree_line(
                &format!(
                    "{}{}{} ({}) {}",
                    prefix,
                    if is_last { "└──" } else { "├──" },
                    package_text,
                    link_text,
                    circular_warn
                )
                .trim_end()
                .to_string(),
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
        let io = self.inner.get_io();
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
