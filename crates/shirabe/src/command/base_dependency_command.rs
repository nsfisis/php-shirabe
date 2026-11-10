//! ref: composer/src/Composer/Command/BaseDependencyCommand.php sha1:b457d7e058eb7787327ee2571c72636c58fe09ac

use crate::command::BaseCommand;
use crate::composer::composer_full;
use crate::io::{IOInterfaceImmutable, IOInterfaceWrite};
use crate::package::version::VersionParser;
use crate::package::{
    Package, PackageHandle, PackageInterfaceHandle, RootPackage, RootPackageInterfaceHandle,
};
use crate::plugin::{CommandEvent, PluginEvents};
use crate::repository::{
    CompositeRepository, DependentsEntry, FindPackageConstraint, InstalledArrayRepository,
    InstalledRepository, NeedleInput, PlatformRepository, RepositoryFactory, RepositoryInterface,
    RepositoryInterfaceHandle, RootPackageRepository,
};
use crate::util::PackageInfo;
use bstr::{B, BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, ToPhpString, UnexpectedValueException, bformat, php_str,
    rtrim,
};
use shirabe_semver::constraint::{AnyConstraint, Bound};
use shirabe_symfony_console::formatter::{OutputFormatter, OutputFormatterStyle};
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName};
use shirabe_symfony_console::output::OutputInterface;

pub const ARGUMENT_PACKAGE: &BStr = php_str(b"package");
pub const ARGUMENT_CONSTRAINT: &BStr = php_str(b"version");
pub const OPTION_RECURSIVE: &BStr = php_str(b"recursive");
pub const OPTION_TREE: &BStr = php_str(b"tree");

pub trait BaseDependencyCommand: BaseCommand {
    const ARGUMENT_PACKAGE: &'static BStr = ARGUMENT_PACKAGE;
    const ARGUMENT_CONSTRAINT: &'static BStr = ARGUMENT_CONSTRAINT;
    const OPTION_RECURSIVE: &'static BStr = OPTION_RECURSIVE;
    const OPTION_TREE: &'static BStr = OPTION_TREE;

    fn colors(&self) -> std::cell::Ref<'_, Vec<BString>>;
    fn set_colors(&self, colors: Vec<BString>);

    fn do_execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        inverted: bool,
    ) -> anyhow::Result<i64> {
        // Emit command event on startup
        let composer = self.require_composer(None, None)?;
        let command_event = CommandEvent::new(
            PluginEvents::COMMAND,
            self.get_name().unwrap_or_default(),
            input.clone(),
            output.clone(),
        );
        let command_event_name = command_event.get_name().to_owned();
        let event_dispatcher = composer.borrow_partial().get_event_dispatcher();
        event_dispatcher.borrow_mut().dispatch(
            Some(command_event_name.as_ref()),
            Some(std::rc::Rc::new(std::cell::RefCell::new(command_event))),
        )?;
        let composer = composer_full(&composer);

        let mut repos: Vec<RepositoryInterfaceHandle> = vec![RepositoryInterfaceHandle::new(
            RootPackageRepository::new(RootPackageInterfaceHandle::dup(composer.get_package())),
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
                return Err(UnexpectedValueException::new(BString::from(
                    "A valid composer.lock file is required to run this command with --locked",
                ))
                .into());
            }

            repos.push(locker.get_locked_repository(true)?.into());
            let platform_overrides: IndexMap<BString, PhpMixed> = locker
                .get_platform_overrides()?
                .into_iter()
                .map(|(k, v)| (k, PhpMixed::string(v)))
                .collect();
            repos.push(RepositoryInterfaceHandle::new(PlatformRepository::new(
                vec![],
                platform_overrides,
                None,
                None,
            )?));
        } else {
            let repository_manager = composer.get_repository_manager().clone();
            let repository_manager = repository_manager.borrow();
            let local_repo = repository_manager.get_local_repository();
            let root_pkg = composer.get_package();

            if local_repo.get_packages()?.is_empty()
                && (!root_pkg.get_requires().is_empty() || !root_pkg.get_dev_requires().is_empty())
            {
                output.borrow().writeln(
                    &[BString::from("<warning>No dependencies installed. Try running shirabe install or update, or use --locked.</warning>")],
                    shirabe_symfony_console::output::OUTPUT_NORMAL,
                );

                return Ok(1);
            }

            repos.push(local_repo);

            let platform_overrides: IndexMap<BString, PhpMixed> = composer
                .get_config()
                .borrow()
                .get_map_or_empty("platform")?
                .into_iter()
                .collect();
            repos.push(RepositoryInterfaceHandle::new(PlatformRepository::new(
                vec![],
                platform_overrides,
                None,
                None,
            )?));
        }

        let mut installed_repo = InstalledRepository::new(repos);

        let needle = BString::from(
            input
                .borrow()
                .get_argument(Self::ARGUMENT_PACKAGE)?
                .as_string()
                .unwrap_or_default(),
        );
        let text_constraint: BString = if input.borrow().has_argument(Self::ARGUMENT_CONSTRAINT) {
            BString::from(
                input
                    .borrow()
                    .get_argument(Self::ARGUMENT_CONSTRAINT)?
                    .as_string()
                    .unwrap_or(BStr::new("*")),
            )
        } else {
            BString::from("*")
        };

        let packages =
            installed_repo.find_packages_with_replacers_and_providers(BStr::new(&needle), None)?;
        if packages.is_empty() {
            return Err(InvalidArgumentException::new(bformat!(
                "Could not find package \"{needle}\" in your project"
            ))
            .into());
        }

        let matched_package = installed_repo.find_package(
            BStr::new(&needle),
            FindPackageConstraint::String(text_constraint.clone()),
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
                    BStr::new(&needle),
                    FindPackageConstraint::String(text_constraint.clone()),
                )? {
                    installed_repo.add_repository(RepositoryInterfaceHandle::new(
                        InstalledArrayRepository::new_with_packages(vec![
                            PackageInterfaceHandle::dup(&r#match),
                        ])?,
                    ));
                } else if PlatformRepository::is_platform_package(&needle) {
                    let parser = VersionParser::new();
                    let platform_constraint = parser.parse_constraints(&text_constraint)?;
                    if platform_constraint.get_lower_bound() != Bound::zero() {
                        let lower_bound = platform_constraint.get_lower_bound();
                        let version = BString::from(lower_bound.get_version());
                        let temp_platform_pkg =
                            Package::new(needle.clone(), version.clone(), version);
                        installed_repo.add_repository(RepositoryInterfaceHandle::new(
                            InstalledArrayRepository::new_with_packages(vec![
                                PackageHandle::from_package(temp_platform_pkg).into(),
                            ])?,
                        ));
                    }
                } else {
                    self.get_io().write_error(bformat!("<error>Package \"{needle}\" could not be found with constraint \"{text_constraint}\", results below will most likely be incomplete.</error>"));
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
                self.get_io().write_error(bformat!("<info>Package \"{needle} {text_constraint}\" found in version \"{}\"{extra_notice}.</info>", matched.get_pretty_version()));
            }
            Some(matched) if inverted => {
                self.get_io().write(bformat!("<comment>Package \"{needle}\" {} is already installed! To find out why, run `shirabe why {needle}`</comment>", matched.get_pretty_version()));

                return Ok(0);
            }
            Some(_) => {}
        }

        let mut needles = vec![needle.clone()];
        if inverted {
            for package in &packages {
                needles.extend(
                    package
                        .get_replaces()
                        .values()
                        .map(|link| BString::from(link.get_target())),
                );
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
            NeedleInput::Multiple(needles),
            constraint,
            inverted,
            recursive,
            None,
        )?;
        if results.is_empty() {
            let extra = if has_constraint {
                bformat!(
                    " in versions {}matching {text_constraint}",
                    if inverted { "not " } else { "" }
                )
            } else {
                BString::default()
            };
            self.get_io().write_error(bformat!(
                "<info>There is no installed package depending on \"{needle}\"{extra}"
            ));
            r#return = if inverted { 0 } else { 1 };
        } else if render_tree {
            self.init_styles(output);
            let root = &packages[0];
            let description = root
                .as_complete()
                .and_then(|c| c.get_description())
                .unwrap_or_default();
            self.get_io().write(bformat!(
                "<info>{}</info> {} {description}",
                root.get_pretty_name(),
                root.get_pretty_version()
            ));
            self.print_tree(&results, BStr::new(""), 1)?;
        } else {
            self.print_table(output, results)?;
        }

        if inverted
            && input.borrow().has_argument(Self::ARGUMENT_CONSTRAINT)
            && !PlatformRepository::is_platform_package(&needle)
        {
            let mut composer_command = "update";

            for root_requirement in composer.get_package().get_requires().values() {
                if root_requirement.get_target() == needle {
                    composer_command = "require";
                    break;
                }
            }

            for root_requirement in composer.get_package().get_dev_requires().values() {
                if root_requirement.get_target() == needle {
                    composer_command = "require --dev";
                    break;
                }
            }

            self.get_io().write_error(bformat!("Not finding what you were looking for? Try calling `shirabe {composer_command} \"{needle}:{text_constraint}\" --dry-run` to get another view on the problem."));
        }

        Ok(r#return)
    }

    fn print_table(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        results: Vec<DependentsEntry>,
    ) -> anyhow::Result<()> {
        let mut table: Vec<Vec<BString>> = vec![];
        let mut doubles: IndexMap<BString, bool> = IndexMap::new();
        let mut results = results;
        loop {
            if results.is_empty() {
                break;
            }
            let mut queue: Vec<DependentsEntry> = vec![];
            let mut rows: Vec<Vec<BString>> = vec![];
            for DependentsEntry(package, link, children) in results {
                let unique = link.to_php_string();
                if doubles.contains_key(&unique) {
                    continue;
                }
                doubles.insert(unique.clone(), true);
                let version = if package.get_pretty_version() == RootPackage::DEFAULT_PRETTY_VERSION
                {
                    BString::from("-")
                } else {
                    package.get_pretty_version()
                };
                let package_url = PackageInfo::get_view_source_or_homepage_url(package.clone());
                let name_with_link = match &package_url {
                    Some(url) => bformat!(
                        "<href={}>{}</>",
                        OutputFormatter::escape(url).expect("OutputFormatter::escape never fails"),
                        package.get_pretty_name()
                    ),
                    None => package.get_pretty_name(),
                };
                rows.push(vec![
                    name_with_link,
                    version,
                    BString::from(link.get_description()),
                    bformat!("{} ({})", link.get_target(), link.get_pretty_constraint()?),
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
        self.render_table(table, output)
    }

    fn init_styles(&self, output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) {
        self.set_colors(vec![
            BString::from("green"),
            BString::from("yellow"),
            BString::from("cyan"),
            BString::from("magenta"),
            BString::from("blue"),
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

    fn print_tree(
        &self,
        results: &[DependentsEntry],
        prefix: &BStr,
        level: i64,
    ) -> anyhow::Result<()> {
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
                    BString::default()
                } else {
                    package.get_pretty_version()
                };
            let package_url = PackageInfo::get_view_source_or_homepage_url(package.clone());
            let name_with_link = match &package_url {
                Some(url) => bformat!(
                    "<href={}>{}</>",
                    OutputFormatter::escape(url).expect("OutputFormatter::escape never fails"),
                    package.get_pretty_name()
                ),
                None => package.get_pretty_name(),
            };
            let package_text = rtrim(bformat!(
                "<{color}>{name_with_link}</{color}> {version_text}"
            ));
            let link_text = bformat!(
                "{} <{prev_color}>{}</{prev_color}> {}",
                link.get_description(),
                link.get_target(),
                link.get_pretty_constraint()?
            );
            let circular_warn = if children.is_none() {
                "(circular dependency aborted here)"
            } else {
                ""
            };
            self.write_tree_line(BStr::new(&rtrim(bformat!(
                "{prefix}{}{package_text} ({link_text}) {circular_warn}",
                if is_last { "└──" } else { "├──" }
            ))));
            if let Some(children_vec) = children {
                self.print_tree(
                    children_vec,
                    bformat!("{prefix}{}", if is_last { "   " } else { "│  " }).as_bstr(),
                    level + 1,
                )?;
            }
        }

        Ok(())
    }

    fn write_tree_line(&self, line: &BStr) {
        let io = self.get_io();
        if !io.is_decorated() {
            io.write(
                line.replace("└", "`-")
                    .replace("├", "|-")
                    .replace("──", "-")
                    .replace("│", "|"),
            );
        } else {
            io.write(line);
        }
    }
}
