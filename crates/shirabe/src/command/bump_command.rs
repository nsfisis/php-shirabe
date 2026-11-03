//! ref: composer/src/Composer/Command/BumpCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData, CompletionTrait};
use crate::console::input::{InputArgument, InputOption};
use crate::factory::Factory;
use crate::io::{IOInterface, IOInterfaceWrite, io_interface};
use crate::json::{JsonFile, JsonManipulator};
use crate::package::base_package;
use crate::package::version::VersionBumper;
use crate::repository::PlatformRepository;
use crate::util::Filesystem;
use bstr::{B, BStr, BString};
use shirabe_php_shim::{
    PhpMixed, RuntimeException, bconcat, file_get_contents, file_put_contents, impl_php_class,
    is_writable, php_regex, preg_is_match, preg_replace, strtolower,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName};
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct BumpCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(BumpCommand, r"Composer\Command\BumpCommand");

impl Default for BumpCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl BumpCommand {
    const ERROR_GENERIC: i64 = 1;
    const ERROR_LOCK_OUTDATED: i64 = 2;

    pub fn new() -> Self {
        let command = BumpCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("BumpCommand::configure uses static, valid metadata");
        command
    }

    pub fn do_bump(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        dev_only: bool,
        no_dev_only: bool,
        dry_run: bool,
        packages_filter: Vec<BString>,
        dev_only_flag_hint: BString,
    ) -> anyhow::Result<i64> {
        let composer_json_path = Factory::get_composer_file()?;

        if !Filesystem::is_readable(&composer_json_path) {
            io.write_error3(
                bconcat!("<error>", composer_json_path, " is not readable.</error>"),
                true,
                io_interface::NORMAL,
            );
            return Ok(Self::ERROR_GENERIC);
        }

        let composer_json = JsonFile::new(composer_json_path.clone(), None, None)?;
        let contents = match file_get_contents(composer_json.get_path()) {
            Ok(c) => c,
            Err(_) => {
                io.write_error3(
                    bconcat!("<error>", composer_json_path, " is not readable.</error>"),
                    true,
                    io_interface::NORMAL,
                );
                return Ok(Self::ERROR_GENERIC);
            }
        };

        if !is_writable(&composer_json_path)
            && file_put_contents(&composer_json_path, &contents).is_none()
        {
            io.write_error3(
                bconcat!("<error>", composer_json_path, " is not writable.</error>"),
                true,
                io_interface::NORMAL,
            );
            return Ok(Self::ERROR_GENERIC);
        }

        let composer = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer);
        let has_lock_file_disabled = !composer.get_config().borrow().has("lock")
            || composer
                .get_config()
                .borrow_mut()
                .get("lock")
                .as_bool()
                .unwrap_or(true);
        let repo: crate::repository::RepositoryInterfaceHandle = if !has_lock_file_disabled {
            composer
                .get_locker()
                .borrow_mut()
                .get_locked_repository(true)?
                .into()
        } else if composer.get_locker().borrow_mut().is_locked() {
            if !composer.get_locker().borrow_mut().is_fresh()? {
                io.write_error3(
                    "<error>The lock file is not up to date with the latest changes in composer.json. Run the appropriate `update` to fix that before you use the `bump` command.</error>",
                    true,
                    io_interface::NORMAL,
                );
                return Ok(Self::ERROR_LOCK_OUTDATED);
            }
            composer
                .get_locker()
                .borrow_mut()
                .get_locked_repository(true)?
                .into()
        } else {
            composer
                .get_repository_manager()
                .borrow()
                .get_local_repository()
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
                .is_some_and(|m| m.contains_key(B("type")))
            {
                io.write_error3(
                    "<warning>If your package is not a library, you can explicitly specify the \"type\" by using \"shirabe config type project\".</warning>",
                    true,
                    io_interface::NORMAL,
                );
                io.write_error3(
                    bconcat!(
                        "<warning>Alternatively you can use ",
                        dev_only_flag_hint,
                        " to only bump dependencies within \"require-dev\".</warning>"
                    ),
                    true,
                    io_interface::NORMAL,
                );
            }
        }

        let bumper = VersionBumper;
        let mut tasks = indexmap::IndexMap::new();
        if !dev_only {
            tasks.insert("require", (*composer.get_package().get_requires()).clone());
        }
        if !no_dev_only {
            tasks.insert(
                "require-dev",
                (*composer.get_package().get_dev_requires()).clone(),
            );
        }

        let packages_filter = if !packages_filter.is_empty() {
            let packages_filter: Vec<BString> = packages_filter
                .iter()
                .map(|constraint| preg_replace(php_regex!(r"{[:= ].+}"), "", constraint))
                .collect();
            let unique_lower: Vec<BString> = packages_filter
                .iter()
                .map(strtolower)
                .collect::<indexmap::IndexSet<_>>()
                .into_iter()
                .collect();
            let pattern =
                base_package::package_names_to_regexp(&unique_lower, BStr::new("{^(?:%s)$}iD"));
            for (key, reqs) in tasks.iter_mut() {
                reqs.retain(|pkg_name, _| preg_is_match(&pattern, pkg_name));
            }
            packages_filter
        } else {
            packages_filter
        };

        let mut updates: indexmap::IndexMap<&BStr, indexmap::IndexMap<BString, BString>> =
            indexmap::IndexMap::new();
        for (key, reqs) in &tasks {
            for (pkg_name, link) in reqs.iter() {
                if PlatformRepository::is_platform_package(pkg_name) {
                    continue;
                }
                let current_constraint = link.get_pretty_constraint();

                let package_opt = repo.find_package(
                    bstr::BStr::new(pkg_name),
                    crate::repository::FindPackageConstraint::String(BString::from("*")),
                )?;
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
                    .entry(BStr::new(*key))
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
                        .entry(bconcat!(key))
                        .or_insert_with(|| PhpMixed::Array(indexmap::IndexMap::new()));
                    if let PhpMixed::Array(map) = section {
                        map.insert(package.clone(), PhpMixed::string(version.clone()));
                    }
                }
            }
            composer_json.write(PhpMixed::Array(composer_definition))?;
        }

        let change_count: usize = updates.values().map(|m| m.len()).sum();
        if change_count > 0 {
            if dry_run {
                io.write(bconcat!(
                    "<info>",
                    composer_json_path,
                    " would be updated with:</info>"
                ));
                for (require_type, packages) in &updates {
                    for (package, version) in packages {
                        io.write(bconcat!(
                            "<info> - ",
                            *require_type,
                            ".",
                            package,
                            ": ",
                            version,
                            "</info>"
                        ));
                    }
                }
            } else {
                io.write(bconcat!(
                    "<info>",
                    composer_json_path,
                    " has been updated (",
                    change_count,
                    " changes).</info>"
                ));
            }
        } else {
            io.write(bconcat!(
                "<info>No requirements to update in ",
                composer_json_path,
                ".</info>"
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
                .update_hash(&composer_json, None)?;
        }

        if dry_run && change_count > 0 {
            return Ok(Self::ERROR_GENERIC);
        }

        Ok(0)
    }

    fn update_file_cleanly(
        &self,
        json: &JsonFile,
        updates: &indexmap::IndexMap<&BStr, indexmap::IndexMap<BString, BString>>,
    ) -> anyhow::Result<bool> {
        let contents = match file_get_contents(json.get_path()) {
            Ok(c) => c,
            Err(_) => {
                return Err(RuntimeException::new(bconcat!(
                    "Unable to read ",
                    json.get_path(),
                    " contents."
                ))
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

        match file_put_contents(json.get_path(), manipulator.get_contents()) {
            Some(_) => Ok(true),
            None => Err(RuntimeException::new(bconcat!(
                "Unable to write new ",
                json.get_path(),
                " contents."
            ))
            .into()),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl Command for BumpCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("bump".into())?;
        self.set_description("Increases the lower limit of your composer.json requirements to the currently installed versions".into());
        self.set_definition(&[
            InputArgument::new5(
                "packages",
                Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
                "Optional package name(s) to restrict which packages are bumped.",
                None,
                self.suggest_root_requirement(),
            )
            .unwrap()
            .into(),
            InputOption::new(
                "dev-only",
                Some("D".into()),
                Some(InputOption::VALUE_NONE),
                "Only bump requirements in \"require-dev\".",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "no-dev-only",
                Some("R".into()),
                Some(InputOption::VALUE_NONE),
                "Only bump requirements in \"require\".",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "dry-run",
                None,
                Some(InputOption::VALUE_NONE),
                "Outputs the packages to bump, but will not execute anything.",
                None,
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
                "The <info>bump</info> command increases the lower limit of your composer.json requirements\n\
                to the currently installed versions. This helps to ensure your dependencies do not\n\
                accidentally get downgraded due to some other conflict, and can slightly improve\n\
                dependency resolution performance as it limits the amount of package versions\n\
                Composer has to look at.\n\n\
                Running this blindly on libraries is **NOT** recommended as it will narrow down\n\
                your allowed dependencies, which may cause dependency hell for your users.\n\
                Running it with <info>--dev-only</info> on libraries may be fine however as dev requirements\n\
                are local to the library and do not affect consumers of the package.\n".into()
            );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let packages_filter: Vec<bstr::BString> = input
            .borrow()
            .get_argument("packages")?
            .as_array()
            .map(<[bstr::BString]>::to_vec)
            .unwrap_or_default();

        let dev_only = input
            .borrow()
            .get_option("dev-only")?
            .as_bool()
            .unwrap_or(false);
        let no_dev_only = input
            .borrow()
            .get_option("no-dev-only")?
            .as_bool()
            .unwrap_or(false);
        let dry_run = input
            .borrow()
            .get_option("dry-run")?
            .as_bool()
            .unwrap_or(false);
        let io = self.get_io().clone();
        self.do_bump(
            io,
            dev_only,
            no_dev_only,
            dry_run,
            packages_filter,
            BString::from("--dev-only"),
        )
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
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for BumpCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
