//! ref: composer/src/Composer/Command/ValidateCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

use crate::advisory::AuditConfig;
use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::loader::ValidatingArrayLoader;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::util::ConfigValidator;
use crate::util::Filesystem;

#[derive(Debug)]
pub struct ValidateCommand {
    base_command_data: BaseCommandData,
}

impl Default for ValidateCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidateCommand {
    pub fn new() -> Self {
        let mut command = ValidateCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("ValidateCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for ValidateCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        self.set_name("validate")?;
        self.set_description("Validates a composer.json and composer.lock");
        self.set_definition(&[
            InputOption::new(
                "no-check-all",
                None,
                Some(InputOption::VALUE_NONE),
                "Do not validate requires for overly strict/loose constraints",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "check-lock",
                None,
                Some(InputOption::VALUE_NONE),
                "Check if lock file is up to date (even when config.lock is false)",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "no-check-lock",
                None,
                Some(InputOption::VALUE_NONE),
                "Do not check if lock file is up to date",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "no-check-publish",
                None,
                Some(InputOption::VALUE_NONE),
                "Do not check for publish errors",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "no-check-version",
                None,
                Some(InputOption::VALUE_NONE),
                "Do not report a warning if the version field is present",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "with-dependencies",
                Some(shirabe_php_shim::PhpMixed::String("A".to_string())),
                Some(InputOption::VALUE_NONE),
                "Also validate the composer.json of all installed dependencies",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "strict",
                None,
                Some(InputOption::VALUE_NONE),
                "Return a non-zero exit code for warnings as well as errors",
                None,
            )
            .unwrap()
            .into(),
            InputArgument::new(
                "file",
                Some(InputArgument::OPTIONAL),
                "path to composer.json file",
                None,
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "The validate command validates a given composer.json and composer.lock\n\n\
            Exit codes in case of errors are:\n\
            1 validation warning(s), only when --strict is given\n\
            2 validation error(s)\n\
            3 file unreadable or missing\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#validate",
        );
        Ok(())
    }

    fn execute(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let file = input
            .borrow()
            .get_argument("file")?
            .as_string()
            .map(|s| s.to_string())
            .map(Ok)
            .unwrap_or_else(Factory::get_composer_file)?;
        let io = self.get_io().clone();

        if !std::path::Path::new(&file).exists() {
            io.write_error(&format!("<error>{} not found.</error>", file));
            return Ok(3);
        }
        if !Filesystem::is_readable(&file) {
            io.write_error(&format!("<error>{} is not readable.</error>", file));
            return Ok(3);
        }

        let validator = ConfigValidator::new(io.clone());
        let check_all = if input
            .borrow()
            .get_option("no-check-all")?
            .as_bool()
            .unwrap_or(false)
        {
            0
        } else {
            ValidatingArrayLoader::CHECK_ALL
        };
        let check_publish = !input
            .borrow()
            .get_option("no-check-publish")?
            .as_bool()
            .unwrap_or(false);
        let check_lock = !input
            .borrow()
            .get_option("no-check-lock")?
            .as_bool()
            .unwrap_or(false);
        let check_version = if input
            .borrow()
            .get_option("no-check-version")?
            .as_bool()
            .unwrap_or(false)
        {
            0
        } else {
            ConfigValidator::CHECK_VERSION
        };
        let is_strict = input
            .borrow()
            .get_option("strict")?
            .as_bool()
            .unwrap_or(false);
        let (mut errors, mut publish_errors, mut warnings) =
            validator.validate(&file, check_all, check_version);

        let mut lock_errors: Vec<String> = vec![];
        let composer =
            self.create_composer_instance(input.clone(), io.clone(), None, false, None)?;
        let mut composer = crate::command::composer_full_mut(&composer);
        let check_lock = (check_lock
            && composer
                .get_config()
                .borrow_mut()
                .get("lock")
                .as_bool()
                .unwrap_or(true))
            || input
                .borrow()
                .get_option("check-lock")?
                .as_bool()
                .unwrap_or(false);
        let locker = composer.get_locker().clone();
        let mut locker = locker.borrow_mut();
        if locker.is_locked() && !locker.is_fresh()? {
            lock_errors.push("- The lock file is not up to date with the latest changes in composer.json, it is recommended that you run `composer update` or `composer update <package name>`.".to_string());
        }

        if locker.is_locked() {
            lock_errors
                .extend(locker.get_missing_requirement_info(composer.get_package().clone(), true)?);
        }

        self.output_result(
            io.clone(),
            &file,
            &mut errors,
            &mut warnings,
            check_publish,
            &mut publish_errors,
            check_lock,
            &mut lock_errors,
            true,
        );

        let exit_code = if !errors.is_empty() {
            2
        } else if is_strict && !warnings.is_empty() {
            1
        } else {
            0
        };

        let mut exit_code = exit_code;

        if input
            .borrow()
            .get_option("with-dependencies")?
            .as_bool()
            .unwrap_or(false)
        {
            let packages = composer
                .get_repository_manager()
                .borrow()
                .get_local_repository()
                .get_packages()?;
            for package in packages {
                let path = composer
                    .get_installation_manager()
                    .borrow_mut()
                    .get_install_path(package.clone());
                let path = match path {
                    Some(p) => p,
                    None => continue,
                };
                let dep_file = format!("{}/composer.json", path);
                if std::path::Path::new(&path).is_dir() && std::path::Path::new(&dep_file).exists()
                {
                    let (mut dep_errors, mut dep_publish_errors, mut dep_warnings) =
                        validator.validate(&dep_file, check_all, check_version);

                    self.output_result(
                        io.clone(),
                        &package.get_pretty_name(),
                        &mut dep_errors,
                        &mut dep_warnings,
                        check_publish,
                        &mut dep_publish_errors,
                        false,
                        &mut vec![],
                        false,
                    );

                    let dep_code = if !dep_errors.is_empty() {
                        2
                    } else if is_strict && !dep_warnings.is_empty() {
                        1
                    } else {
                        0
                    };
                    exit_code = exit_code.max(dep_code);
                }
            }
        }

        // TODO(plugin): dispatch CommandEvent
        let command_event = CommandEvent::new(PluginEvents::COMMAND, "validate", input, output);
        let event_code = composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None)?;

        Ok(exit_code.max(event_code))
    }

    fn initialize(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ValidateCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl ValidateCommand {
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    fn output_result(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        name: &str,
        errors: &mut Vec<String>,
        warnings: &mut Vec<String>,
        check_publish: bool,
        publish_errors: &mut Vec<String>,
        check_lock: bool,
        lock_errors: &mut Vec<String>,
        print_schema_url: bool,
    ) {
        let mut do_print_schema_url = false;

        if !errors.is_empty() {
            io.write_error(&format!(
                "<error>{} is invalid, the following errors/warnings were found:</error>",
                name
            ));
        } else if !publish_errors.is_empty() && check_publish {
            io.write_error(&format!(
                "<info>{} is valid for simple usage with Composer but has</info>",
                name
            ));
            io.write_error(
                "<info>strict errors that make it unable to be published as a package</info>",
            );
            do_print_schema_url = print_schema_url;
        } else if !warnings.is_empty() {
            io.write_error(&format!(
                "<info>{} is valid, but with a few warnings</info>",
                name
            ));
            do_print_schema_url = print_schema_url;
        } else if !lock_errors.is_empty() {
            io.write(&format!(
                "<info>{} is valid but your composer.lock has some {}</info>",
                name,
                if check_lock { "errors" } else { "warnings" }
            ));
        } else {
            io.write(&format!("<info>{} is valid</info>", name));
        }

        if do_print_schema_url {
            io.write_error("<warning>See https://getcomposer.org/doc/04-schema.md for details on the schema</warning>");
        }

        if !errors.is_empty() {
            *errors = errors.iter().map(|e| format!("- {}", e)).collect();
            errors.insert(0, "# General errors".to_string());
        }
        if !warnings.is_empty() {
            *warnings = warnings.iter().map(|w| format!("- {}", w)).collect();
            warnings.insert(0, "# General warnings".to_string());
        }

        let mut extra_warnings: Vec<String> = vec![];

        if !publish_errors.is_empty() && check_publish {
            *publish_errors = publish_errors.iter().map(|e| format!("- {}", e)).collect();
            publish_errors.insert(0, "# Publish errors".to_string());
            errors.append(publish_errors);
        }

        if !lock_errors.is_empty() {
            if check_lock {
                lock_errors.insert(0, "# Lock file errors".to_string());
                errors.append(lock_errors);
            } else {
                lock_errors.insert(0, "# Lock file warnings".to_string());
                extra_warnings.append(lock_errors);
            }
        }

        let all_warnings: Vec<String> = warnings.iter().cloned().chain(extra_warnings).collect();

        for msg in errors.iter() {
            if msg.starts_with('#') {
                io.write_error(&format!("<error>{}</error>", msg));
            } else {
                io.write_error(msg);
            }
        }
        for msg in &all_warnings {
            if msg.starts_with('#') {
                io.write_error(&format!("<warning>{}</warning>", msg));
            } else {
                io.write_error(msg);
            }
        }
    }
}
