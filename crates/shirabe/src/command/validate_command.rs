//! ref: composer/src/Composer/Command/ValidateCommand.php

use anyhow::Result;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::command::command::CommandBase;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;

use crate::command::base_command::BaseCommand;
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::package::loader::validating_array_loader::ValidatingArrayLoader;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::util::config_validator::ConfigValidator;
use crate::util::filesystem::Filesystem;

#[derive(Debug)]
pub struct ValidateCommand {
    inner: CommandBase,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl ValidateCommand {
    pub fn configure(&mut self) {
        self.inner
            .set_name("validate")
            .set_description("Validates a composer.json and composer.lock")
            .set_definition(vec![
                InputOption::new(
                    "no-check-all",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Do not validate requires for overly strict/loose constraints",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "check-lock",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Check if lock file is up to date (even when config.lock is false)",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-check-lock",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Do not check if lock file is up to date",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-check-publish",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Do not check for publish errors",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-check-version",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Do not report a warning if the version field is present",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "with-dependencies",
                    Some(shirabe_php_shim::PhpMixed::String("A".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Also validate the composer.json of all installed dependencies",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "strict",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Return a non-zero exit code for warnings as well as errors",
                    None,
                    vec![],
                ),
                InputArgument::new(
                    "file",
                    Some(InputArgument::OPTIONAL),
                    "path to composer.json file",
                    None,
                    vec![],
                ),
            ])
            .set_help(
                "The validate command validates a given composer.json and composer.lock\n\n\
                Exit codes in case of errors are:\n\
                1 validation warning(s), only when --strict is given\n\
                2 validation error(s)\n\
                3 file unreadable or missing\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#validate",
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> Result<i64> {
        let file = input
            .get_argument("file")
            .as_string_opt()
            .map(|s| s.to_string())
            .unwrap_or_else(|| Factory::get_composer_file());
        let io = self.inner.get_io();

        if !std::path::Path::new(&file).exists() {
            io.write_error(&format!("<error>{} not found.</error>", file));
            return Ok(3);
        }
        if !Filesystem::is_readable(&file) {
            io.write_error(&format!("<error>{} is not readable.</error>", file));
            return Ok(3);
        }

        let validator = ConfigValidator::new(io);
        let check_all = if input.get_option("no-check-all").as_bool().unwrap_or(false) {
            0
        } else {
            ValidatingArrayLoader::CHECK_ALL
        };
        let check_publish = !input
            .get_option("no-check-publish")
            .as_bool()
            .unwrap_or(false);
        let check_lock = !input.get_option("no-check-lock").as_bool().unwrap_or(false);
        let check_version = if input
            .get_option("no-check-version")
            .as_bool()
            .unwrap_or(false)
        {
            0
        } else {
            ConfigValidator::CHECK_VERSION
        };
        let is_strict = input.get_option("strict").as_bool().unwrap_or(false);
        let (mut errors, mut publish_errors, mut warnings) =
            validator.validate(&file, check_all, check_version)?;

        let mut lock_errors: Vec<String> = vec![];
        let composer = self.inner.create_composer_instance(input, io, vec![])?;
        let check_lock = (check_lock
            && composer.get_config().get("lock").as_bool().unwrap_or(true))
            || input.get_option("check-lock").as_bool().unwrap_or(false);
        let locker = composer.get_locker();
        if locker.is_locked() && !locker.is_fresh() {
            lock_errors.push("- The lock file is not up to date with the latest changes in composer.json, it is recommended that you run `composer update` or `composer update <package name>`.".to_string());
        }

        if locker.is_locked() {
            lock_errors.extend(locker.get_missing_requirement_info(composer.get_package(), true));
        }

        self.output_result(
            io,
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
            .get_option("with-dependencies")
            .as_bool()
            .unwrap_or(false)
        {
            let local_repo = composer.get_repository_manager().get_local_repository();
            for package in local_repo.get_packages() {
                let path = composer
                    .get_installation_manager()
                    .get_install_path(package.as_ref());
                let path = match path {
                    Some(p) => p,
                    None => continue,
                };
                let dep_file = format!("{}/composer.json", path);
                if std::path::Path::new(&path).is_dir() && std::path::Path::new(&dep_file).exists()
                {
                    let (mut dep_errors, mut dep_publish_errors, mut dep_warnings) =
                        validator.validate(&dep_file, check_all, check_version)?;

                    self.output_result(
                        io,
                        package.get_pretty_name(),
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
        let command_event = CommandEvent::new(
            PluginEvents::COMMAND.to_string(),
            "validate".to_string(),
            Box::new(input),
            Box::new(output),
            vec![],
            vec![],
        );
        let event_code = composer
            .get_event_dispatcher()
            .dispatch(command_event.get_name(), &command_event);

        Ok(exit_code.max(event_code))
    }

    fn output_result(
        &self,
        io: &dyn IOInterface,
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
            errors.extend(publish_errors.drain(..));
        }

        if !lock_errors.is_empty() {
            if check_lock {
                lock_errors.insert(0, "# Lock file errors".to_string());
                errors.extend(lock_errors.drain(..));
            } else {
                lock_errors.insert(0, "# Lock file warnings".to_string());
                extra_warnings.extend(lock_errors.drain(..));
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

impl BaseCommand for ValidateCommand {
    fn inner(&self) -> &CommandBase {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut CommandBase {
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

impl Command for ValidateCommand {}
