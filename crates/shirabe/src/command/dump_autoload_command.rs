//! ref: composer/src/Composer/Command/DumpAutoloadCommand.php

use anyhow::Result;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, file_exists};

use crate::command::base_command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::Composer;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;

#[derive(Debug)]
pub struct DumpAutoloadCommand {
    base_command_data: BaseCommandData,
}

impl DumpAutoloadCommand {
    pub fn configure(&mut self) {
        self
            .set_name("dump-autoload")
            .set_aliases(&["dumpautoload".to_string()])
            .set_description("Dumps the autoloader")
            .set_definition(&[
                InputOption::new("optimize", Some(PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Optimizes PSR0 and PSR4 packages to be loaded with classmaps too, good for production.", None).unwrap().into(),
        InputOption::new("classmap-authoritative", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize`.", None).unwrap().into(),
        InputOption::new("apcu", None, Some(InputOption::VALUE_NONE), "Use APCu to cache found/not-found classes.", None).unwrap().into(),
        InputOption::new("apcu-prefix", None, Some(InputOption::VALUE_REQUIRED), "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu", None).unwrap().into(),
        InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the operations but will not execute anything.", None).unwrap().into(),
        InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "Enables autoload-dev rules. Composer will by default infer this automatically according to the last install or update --no-dev state.", None).unwrap().into(),
        InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables autoload-dev rules. Composer will by default infer this automatically according to the last install or update --no-dev state.", None).unwrap().into(),
        InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None).unwrap().into(),
        InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None).unwrap().into(),
        InputOption::new("strict-psr", None, Some(InputOption::VALUE_NONE), "Return a failed status code (1) if PSR-4 or PSR-0 mapping errors are present. Requires --optimize to work.", None).unwrap().into(),
        InputOption::new("strict-ambiguous", None, Some(InputOption::VALUE_NONE), "Return a failed status code (2) if the same class is found in multiple files. Requires --optimize to work.", None).unwrap().into(),
            ])
            .set_help(
                "<info>php composer.phar dump-autoload</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#dump-autoload-dumpautoload"
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> Result<i64> {
        let composer = self.require_composer(None, None)?;

        // TODO(plugin): dispatch CommandEvent
        let command_event =
            CommandEvent::new(PluginEvents::COMMAND, "dump-autoload", input, output);
        composer
            .get_event_dispatcher()
            .dispatch(Some(command_event.get_name()), None);

        let installation_manager = composer.get_installation_manager();
        let local_repo = composer.get_repository_manager().get_local_repository();
        let package = composer.get_package();
        let config = composer.get_config();

        let mut missing_dependencies = false;
        for local_pkg in local_repo.get_canonical_packages() {
            let install_path = installation_manager.get_install_path(&*local_pkg);
            if install_path.as_deref().is_some_and(|p| !file_exists(p)) {
                missing_dependencies = true;
                self.get_io().write("<warning>Not all dependencies are installed. Make sure to run a \"composer install\" to install missing dependencies</warning>");
                break;
            }
        }

        let optimize = input.get_option("optimize").as_bool().unwrap_or(false)
            || config
                .borrow_mut()
                .get("optimize-autoloader")
                .as_bool()
                .unwrap_or(false);
        let authoritative = input
            .get_option("classmap-authoritative")
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("classmap-authoritative")
                .as_bool()
                .unwrap_or(false);
        let apcu_prefix = input
            .get_option("apcu-prefix")
            .as_string_opt()
            .map(|s| s.to_string());
        let apcu = apcu_prefix.is_some()
            || input.get_option("apcu").as_bool().unwrap_or(false)
            || config
                .borrow_mut()
                .get("apcu-autoloader")
                .as_bool()
                .unwrap_or(false);

        if input.get_option("strict-psr").as_bool().unwrap_or(false) && !optimize && !authoritative
        {
            return Err(InvalidArgumentException {
                message: "--strict-psr mode only works with optimized autoloader, use --optimize or --classmap-authoritative if you want a strict return value.".to_string(),
                code: 0,
            }
            .into());
        }
        if input
            .get_option("strict-ambiguous")
            .as_bool()
            .unwrap_or(false)
            && !optimize
            && !authoritative
        {
            return Err(InvalidArgumentException {
                message: "--strict-ambiguous mode only works with optimized autoloader, use --optimize or --classmap-authoritative if you want a strict return value.".to_string(),
                code: 0,
            }
            .into());
        }

        if authoritative {
            self.get_io()
                .write("<info>Generating optimized autoload files (authoritative)</info>");
        } else if optimize {
            self.get_io()
                .write("<info>Generating optimized autoload files</info>");
        } else {
            self.get_io()
                .write("<info>Generating autoload files</info>");
        }

        let generator = composer.get_autoload_generator();
        if input.get_option("dry-run").as_bool().unwrap_or(false) {
            generator.set_dry_run(true);
        }
        if input.get_option("no-dev").as_bool().unwrap_or(false) {
            generator.set_dev_mode(false);
        }
        if input.get_option("dev").as_bool().unwrap_or(false) {
            if input.get_option("no-dev").as_bool().unwrap_or(false) {
                return Err(InvalidArgumentException {
                    message:
                        "You can not use both --no-dev and --dev as they conflict with each other."
                            .to_string(),
                    code: 0,
                }
                .into());
            }
            generator.set_dev_mode(true);
        }
        generator.set_class_map_authoritative(authoritative);
        generator.set_run_scripts(true);
        generator.set_apcu(apcu, apcu_prefix.as_deref());
        generator.set_platform_requirement_filter(self.get_platform_requirement_filter(input)?);
        let class_map = generator.dump(
            &*config.borrow(),
            &local_repo,
            package,
            installation_manager,
            "composer",
            optimize,
            None,
            composer.get_locker(),
            input
                .get_option("strict-ambiguous")
                .as_bool()
                .unwrap_or(false),
        )?;
        let number_of_classes = class_map.len();

        if authoritative {
            self.get_io().write(&format!("<info>Generated optimized autoload files (authoritative) containing {} classes</info>", number_of_classes));
        } else if optimize {
            self.get_io().write(&format!(
                "<info>Generated optimized autoload files containing {} classes</info>",
                number_of_classes
            ));
        } else {
            self.get_io().write("<info>Generated autoload files</info>");
        }

        if missing_dependencies
            || (input.get_option("strict-psr").as_bool().unwrap_or(false)
                && !class_map.get_psr_violations().is_empty())
        {
            return Ok(1);
        }

        if input
            .get_option("strict-ambiguous")
            .as_bool()
            .unwrap_or(false)
            && !class_map.get_ambiguous_classes(false)?.is_empty()
        {
            return Ok(2);
        }

        Ok(0)
    }
}

impl HasBaseCommandData for DumpAutoloadCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
