//! ref: composer/src/Composer/Command/BumpCommand.php

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{PhpMixed, file_get_contents, file_put_contents, is_writable, strtolower};

use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::json::json_manipulator::JsonManipulator;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::package::version::version_bumper::VersionBumper;
use crate::repository::platform_repository::PlatformRepository;
use crate::util::filesystem::Filesystem;
use crate::util::silencer::Silencer;

#[derive(Debug)]
pub struct BumpCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl CompletionTrait for BumpCommand {}

impl BumpCommand {
    const ERROR_GENERIC: i64 = 1;
    const ERROR_LOCK_OUTDATED: i64 = 2;

    pub fn configure(&mut self) {
        let suggest_root_requirement = self.suggest_root_requirement();
        self.inner
            .set_name("bump")
            .set_description("Increases the lower limit of your composer.json requirements to the currently installed versions")
            .set_definition(vec![
                InputArgument::new(
                    "packages",
                    Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
                    "Optional package name(s) to restrict which packages are bumped.",
                    None,
                    suggest_root_requirement,
                ),
                InputOption::new("dev-only", Some(PhpMixed::String("D".to_string())), Some(InputOption::VALUE_NONE), "Only bump requirements in \"require-dev\".", None, vec![]),
                InputOption::new("no-dev-only", Some(PhpMixed::String("R".to_string())), Some(InputOption::VALUE_NONE), "Only bump requirements in \"require\".", None, vec![]),
                InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the packages to bump, but will not execute anything.", None, vec![]),
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
        &self,
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

        self.do_bump(
            self.inner.get_io(),
            input.get_option("dev-only").as_bool().unwrap_or(false),
            input.get_option("no-dev-only").as_bool().unwrap_or(false),
            input.get_option("dry-run").as_bool().unwrap_or(false),
            packages_filter,
            "--dev-only".to_string(),
        )
    }

    pub fn do_bump(
        &self,
        io: &dyn IOInterface,
        dev_only: bool,
        no_dev_only: bool,
        dry_run: bool,
        packages_filter: Vec<String>,
        dev_only_flag_hint: String,
    ) -> Result<i64> {
        let composer_json_path = Factory::get_composer_file();

        if !Filesystem::is_readable(&composer_json_path) {
            io.write_error(
                PhpMixed::String(format!(
                    "<error>{} is not readable.</error>",
                    composer_json_path
                )),
                true,
                IOInterface::NORMAL,
            );
            return Ok(Self::ERROR_GENERIC);
        }

        let composer_json = JsonFile::new(composer_json_path.clone());
        let contents = match file_get_contents(&composer_json.get_path()) {
            Some(c) => c,
            None => {
                io.write_error(
                    PhpMixed::String(format!(
                        "<error>{} is not readable.</error>",
                        composer_json_path
                    )),
                    true,
                    IOInterface::NORMAL,
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
            io.write_error(
                PhpMixed::String(format!(
                    "<error>{} is not writable.</error>",
                    composer_json_path
                )),
                true,
                IOInterface::NORMAL,
            );
            return Ok(Self::ERROR_GENERIC);
        }

        let composer = self.inner.require_composer()?;
        let has_lock_file_disabled = !composer.get_config().has("lock")
            || composer.get_config().get("lock").as_bool().unwrap_or(true);
        let repo = if !has_lock_file_disabled {
            composer.get_locker().get_locked_repository(true)?
        } else if composer.get_locker().is_locked() {
            if !composer.get_locker().is_fresh() {
                io.write_error(
                    PhpMixed::String(
                        "<error>The lock file is not up to date with the latest changes in composer.json. Run the appropriate `update` to fix that before you use the `bump` command.</error>".to_string(),
                    ),
                    true,
                    IOInterface::NORMAL,
                );
                return Ok(Self::ERROR_LOCK_OUTDATED);
            }
            composer.get_locker().get_locked_repository(true)?
        } else {
            composer.get_repository_manager().get_local_repository()
        };

        if composer.get_package().get_type() != "project" && !dev_only {
            io.write_error(
                PhpMixed::String(
                    "<warning>Warning: Bumping dependency constraints is not recommended for libraries as it will narrow down your dependencies and may cause problems for your users.</warning>".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );

            let contents_data = composer_json.read()?;
            if !contents_data.contains_key("type") {
                io.write_error(
                    PhpMixed::String(
                        "If your package is not a library, you can explicitly specify the \"type\" by using \"composer config type project\".".to_string(),
                    ),
                    true,
                    IOInterface::NORMAL,
                );
                io.write_error(
                    PhpMixed::String(format!(
                        "<warning>Alternatively you can use {} to only bump dependencies within \"require-dev\".</warning>",
                        dev_only_flag_hint
                    )),
                    true,
                    IOInterface::NORMAL,
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
                    Preg::replace(r"{[:= ].+}", "", constraint.clone())
                        .unwrap_or_else(|_| constraint.clone())
                })
                .collect();
            let mut unique_lower: Vec<String> = packages_filter
                .iter()
                .map(|s| strtolower(s))
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            let pattern = BasePackage::package_names_to_regexp(&unique_lower);
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
                let current_constraint = link.get_pretty_constraint();

                let package_opt = repo.find_package(pkg_name, "*");
                let package = match package_opt {
                    None => continue,
                    Some(p) => p,
                };
                let mut package = package;
                while let Some(alias) = package.as_any().downcast_ref::<AliasPackage>() {
                    package = alias.get_alias_of();
                }

                let bumped =
                    bumper.bump_requirement(link.get_constraint().as_ref(), package.as_ref())?;

                if bumped == current_constraint {
                    continue;
                }

                updates
                    .entry(key)
                    .or_default()
                    .insert(pkg_name.clone(), bumped);
            }
        }

        if !dry_run && !self.update_file_cleanly(&composer_json, &updates)? {
            let mut composer_definition = composer_json.read()?;
            for (key, packages) in &updates {
                for (package, version) in packages {
                    composer_definition
                        .entry(key.to_string())
                        .or_insert_with(indexmap::IndexMap::new)
                        .insert(package.clone(), version.clone());
                }
            }
            composer_json.write(composer_definition)?;
        }

        let change_count: usize = updates.values().map(|m| m.len()).sum();
        if change_count > 0 {
            if dry_run {
                io.write(
                    PhpMixed::String(format!(
                        "<info>{} would be updated with:</info>",
                        composer_json_path
                    )),
                    true,
                    IOInterface::NORMAL,
                );
                for (require_type, packages) in &updates {
                    for (package, version) in packages {
                        io.write(
                            PhpMixed::String(format!(
                                "<info> - {}.{}: {}</info>",
                                require_type, package, version
                            )),
                            true,
                            IOInterface::NORMAL,
                        );
                    }
                }
            } else {
                io.write(
                    PhpMixed::String(format!(
                        "<info>{} has been updated ({} changes).</info>",
                        composer_json_path, change_count
                    )),
                    true,
                    IOInterface::NORMAL,
                );
            }
        } else {
            io.write(
                PhpMixed::String(format!(
                    "<info>No requirements to update in {}.</info>",
                    composer_json_path
                )),
                true,
                IOInterface::NORMAL,
            );
        }

        if !dry_run
            && composer.get_locker().is_locked()
            && composer.get_config().get("lock").as_bool().unwrap_or(true)
            && change_count > 0
        {
            composer.get_locker().update_hash(&composer_json)?;
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
                if !manipulator.add_link(key, package, version)? {
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

impl BaseCommand for BumpCommand {
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
