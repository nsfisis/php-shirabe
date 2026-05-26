//! ref: composer/src/Composer/Command/BumpCommand.php

use crate::io::io_interface;
use crate::package::base_package;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{PhpMixed, file_get_contents, file_put_contents, is_writable, strtolower};

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::io::IOInterface;
use crate::json::JsonFile;
use crate::json::JsonManipulator;
use crate::package::AliasPackage;
use crate::package::BasePackage;
use crate::package::version::VersionBumper;
use crate::repository::PlatformRepository;
use crate::util::Filesystem;
use crate::util::Silencer;

#[derive(Debug)]
pub struct BumpCommand {
    base_command_data: BaseCommandData,
}

impl BumpCommand {
    const ERROR_GENERIC: i64 = 1;
    const ERROR_LOCK_OUTDATED: i64 = 2;

    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_root_requirement() for `packages` argument
        self
            .set_name("bump")
            .set_description("Increases the lower limit of your composer.json requirements to the currently installed versions")
            .set_definition (&[
                InputArgument::new("packages",Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),"Optional package name(s) to restrict which packages are bumped.",None,).unwrap().into(),InputOption::new("dev-only", Some(PhpMixed::String("D".to_string())), Some(InputOption::VALUE_NONE), "Only bump requirements in \"require-dev\".", None).unwrap().into(),
                    InputOption::new("no-dev-only", Some(PhpMixed::String("R".to_string())), Some(InputOption::VALUE_NONE), "Only bump requirements in \"require\".", None).unwrap().into(),
        InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the packages to bump, but will not execute anything.", None).unwrap().into(),
            ])
            .set_help(
                "The <info>bump</info> command increases the lower limit of your composer.json requirements\n\
                to the currently installed versions. This helps to ensure your dependencies do not\n\
                accidentally get downgraded due to some other conflict, and can slightly improve\n\
                dependency resolution performance as it limits the amount of package versions\n\
                Composer has to look at.\n\n\
                Running this blindly on libraries is **NOT** recommended as it will narrow down\n\
                your allowed dependencies, which may cause dependency hell for your users.\n\
                Running it with <info>--dev-only</info> on libraries may be fine however as dev requirements\n\
                are local to the library and do not affect consumers of the package.\n"
            );
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let packages_filter: Vec<String> = input
            .get_argument("packages")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let dev_only = input.get_option("dev-only").as_bool().unwrap_or(false);
        let no_dev_only = input.get_option("no-dev-only").as_bool().unwrap_or(false);
        let dry_run = input.get_option("dry-run").as_bool().unwrap_or(false);
        // TODO(phase-b): do_bump expects &dyn IOInterface but get_io() requires &mut self; needs IO sharing refactor
        let io_ref: &dyn IOInterface = todo!("share IOInterface across calls in do_bump");
        self.do_bump(
            io_ref,
            dev_only,
            no_dev_only,
            dry_run,
            packages_filter,
            "--dev-only".to_string(),
        )
    }

    pub fn do_bump(
        &mut self,
        io: &dyn IOInterface,
        dev_only: bool,
        no_dev_only: bool,
        dry_run: bool,
        packages_filter: Vec<String>,
        dev_only_flag_hint: String,
    ) -> Result<i64> {
        let composer_json_path = Factory::get_composer_file()?;

        if !Filesystem::is_readable(&composer_json_path) {
            io.write_error3(
                &format!("<error>{} is not readable.</error>", composer_json_path),
                true,
                io_interface::NORMAL,
            );
            return Ok(Self::ERROR_GENERIC);
        }

        let mut composer_json = JsonFile::new(composer_json_path.clone(), None, None)?;
        let contents = match file_get_contents(&composer_json.get_path()) {
            Some(c) => c,
            None => {
                io.write_error3(
                    &format!("<error>{} is not readable.</error>", composer_json_path),
                    true,
                    io_interface::NORMAL,
                );
                return Ok(Self::ERROR_GENERIC);
            }
        };

        if !is_writable(&composer_json_path)
            && Silencer::call(|| {
                file_put_contents(&composer_json_path, contents.as_bytes())
                    .map(|_| ())
                    .ok_or_else(|| anyhow::anyhow!("file_put_contents failed"))
            })
            .is_err()
        {
            io.write_error3(
                &format!("<error>{} is not writable.</error>", composer_json_path),
                true,
                io_interface::NORMAL,
            );
            return Ok(Self::ERROR_GENERIC);
        }

        let composer = self.require_composer(None, None)?;
        let mut composer = crate::command::composer_full_mut(&composer);
        let has_lock_file_disabled = !composer.get_config().borrow().has("lock")
            || composer
                .get_config()
                .borrow_mut()
                .get("lock")
                .as_bool()
                .unwrap_or(true);
        let repo: Box<dyn crate::repository::RepositoryInterface> = if !has_lock_file_disabled {
            Box::new(
                composer
                    .get_locker()
                    .borrow_mut()
                    .get_locked_repository(true)?,
            )
        } else if composer.get_locker().borrow_mut().is_locked() {
            if !composer.get_locker().borrow_mut().is_fresh()? {
                io.write_error3(
                    "<error>The lock file is not up to date with the latest changes in composer.json. Run the appropriate `update` to fix that before you use the `bump` command.</error>",
                    true,
                    io_interface::NORMAL,
                );
                return Ok(Self::ERROR_LOCK_OUTDATED);
            }
            Box::new(
                composer
                    .get_locker()
                    .borrow_mut()
                    .get_locked_repository(true)?,
            )
        } else {
            // TODO(phase-b): get_local_repository returns &dyn InstalledRepositoryInterface;
            // cloning into an owned Box requires clone_box on that trait.
            composer
                .get_repository_manager()
                .borrow()
                .get_local_repository()
                .clone_box()
        };

        if composer.get_package().get_type() != "project" && !dev_only {
            io.write_error3(
                "<warning>Warning: Bumping dependency constraints is not recommended for libraries as it will narrow down your dependencies and may cause problems for your users.</warning>",
                true,
                io_interface::NORMAL,
            );

            let contents_data = composer_json.read()?;
            if !contents_data
                .as_array()
                .map_or(false, |m| m.contains_key("type"))
            {
                io.write_error3(
                    "If your package is not a library, you can explicitly specify the \"type\" by using \"composer config type project\".",
                    true,
                    io_interface::NORMAL,
                );
                io.write_error3(
                    &format!(
                        "<warning>Alternatively you can use {} to only bump dependencies within \"require-dev\".</warning>",
                        dev_only_flag_hint
                    ),
                    true,
                    io_interface::NORMAL,
                );
            }
        }

        let bumper = VersionBumper;
        let mut tasks = indexmap::IndexMap::new();
        if !dev_only {
            tasks.insert("require", composer.get_package().get_requires());
        }
        if !no_dev_only {
            tasks.insert("require-dev", composer.get_package().get_dev_requires());
        }

        let packages_filter = if !packages_filter.is_empty() {
            let packages_filter: Vec<String> = packages_filter
                .iter()
                .map(|constraint| {
                    Preg::replace(r"{[:= ].+}", "", constraint)
                        .unwrap_or_else(|_| constraint.clone())
                })
                .collect();
            let mut unique_lower: Vec<String> = packages_filter
                .iter()
                .map(|s| strtolower(s))
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            let pattern = base_package::package_names_to_regexp(&unique_lower, "{^(?:%s)$}iD");
            for (key, reqs) in tasks.iter_mut() {
                reqs.retain(|pkg_name, _| Preg::is_match(&pattern, pkg_name).unwrap_or(false));
            }
            packages_filter
        } else {
            packages_filter
        };

        let mut updates: indexmap::IndexMap<&str, indexmap::IndexMap<String, String>> =
            indexmap::IndexMap::new();
        for (key, reqs) in &tasks {
            for (pkg_name, link) in reqs {
                if PlatformRepository::is_platform_package(pkg_name) {
                    continue;
                }
                let current_constraint = link.get_pretty_constraint()?;

                let package_opt = repo.find_package(
                    pkg_name,
                    crate::repository::FindPackageConstraint::String("*".to_string()),
                );
                let mut package = match package_opt {
                    None => continue,
                    Some(p) => p,
                };
                while let Some(alias) = package.as_alias() {
                    package = alias.get_alias_of().into();
                }

                let bumped = bumper.bump_requirement(link.get_constraint(), package.clone())?;

                if bumped == current_constraint {
                    continue;
                }

                updates
                    .entry(*key)
                    .or_default()
                    .insert(pkg_name.clone(), bumped);
            }
        }

        if !dry_run && !self.update_file_cleanly(&composer_json, &updates)? {
            let mut composer_definition = match composer_json.read()? {
                PhpMixed::Array(m) => m,
                _ => indexmap::IndexMap::new(),
            };
            for (key, packages) in &updates {
                for (package, version) in packages {
                    let section = composer_definition
                        .entry(key.to_string())
                        .or_insert_with(|| Box::new(PhpMixed::Array(indexmap::IndexMap::new())));
                    if let PhpMixed::Array(map) = section.as_mut() {
                        map.insert(package.clone(), Box::new(PhpMixed::String(version.clone())));
                    }
                }
            }
            composer_json.write(PhpMixed::Array(composer_definition))?;
        }

        let change_count: usize = updates.values().map(|m| m.len()).sum();
        if change_count > 0 {
            if dry_run {
                io.write(&format!(
                    "<info>{} would be updated with:</info>",
                    composer_json_path
                ));
                for (require_type, packages) in &updates {
                    for (package, version) in packages {
                        io.write(&format!(
                            "<info> - {}.{}: {}</info>",
                            require_type, package, version
                        ));
                    }
                }
            } else {
                io.write(&format!(
                    "<info>{} has been updated ({} changes).</info>",
                    composer_json_path, change_count
                ));
            }
        } else {
            io.write(&format!(
                "<info>No requirements to update in {}.</info>",
                composer_json_path
            ));
        }

        if !dry_run
            && composer.get_locker().borrow_mut().is_locked()
            && composer
                .get_config()
                .borrow_mut()
                .get("lock")
                .as_bool()
                .unwrap_or(true)
            && change_count > 0
        {
            composer
                .get_locker()
                .borrow_mut()
                .update_hash(&composer_json, None::<fn(_) -> _>)?;
        }

        if dry_run && change_count > 0 {
            return Ok(Self::ERROR_GENERIC);
        }

        Ok(0)
    }

    fn update_file_cleanly(
        &self,
        json: &JsonFile,
        updates: &indexmap::IndexMap<&str, indexmap::IndexMap<String, String>>,
    ) -> Result<bool> {
        let contents = match file_get_contents(&json.get_path()) {
            Some(c) => c,
            None => {
                return Err(shirabe_php_shim::RuntimeException {
                    message: format!("Unable to read {} contents.", json.get_path()),
                    code: 0,
                }
                .into());
            }
        };

        let mut manipulator = JsonManipulator::new(contents)?;

        for (key, packages) in updates {
            for (package, version) in packages {
                if !manipulator.add_link(key, package, version, false)? {
                    return Ok(false);
                }
            }
        }

        match file_put_contents(&json.get_path(), manipulator.get_contents().as_bytes()) {
            Some(_) => Ok(true),
            None => Err(shirabe_php_shim::RuntimeException {
                message: format!("Unable to write new {} contents.", json.get_path()),
                code: 0,
            }
            .into()),
        }
    }
}

impl HasBaseCommandData for BumpCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
