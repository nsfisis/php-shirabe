//! ref: composer/src/Composer/Command/DumpAutoloadCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::console::input::InputOption;
use crate::io::IOInterfaceImmutable;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use shirabe_php_shim::{InvalidArgumentException, file_exists, impl_php_class};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct DumpAutoloadCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(DumpAutoloadCommand, r"Composer\Command\DumpAutoloadCommand");

impl Default for DumpAutoloadCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl DumpAutoloadCommand {
    pub fn new() -> Self {
        let command = DumpAutoloadCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("DumpAutoloadCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for DumpAutoloadCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("dump-autoload")?;
        self.set_aliases(vec!["dumpautoload".to_string()])?;
        self.set_description("Dumps the autoloader");
        self.set_definition(&[
            InputOption::new("optimize", Some("o"), Some(InputOption::VALUE_NONE), "Optimizes PSR0 and PSR4 packages to be loaded with classmaps too, good for production.", None).unwrap().into(),
        InputOption::new("classmap-authoritative", Some("a"), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize`.", None).unwrap().into(),
        InputOption::new("apcu", None, Some(InputOption::VALUE_NONE), "Use APCu to cache found/not-found classes.", None).unwrap().into(),
        InputOption::new("apcu-prefix", None, Some(InputOption::VALUE_REQUIRED), "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu", None).unwrap().into(),
        InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the operations but will not execute anything.", None).unwrap().into(),
        InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "Enables autoload-dev rules. Composer will by default infer this automatically according to the last install or update --no-dev state.", None).unwrap().into(),
        InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables autoload-dev rules. Composer will by default infer this automatically according to the last install or update --no-dev state.", None).unwrap().into(),
        InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None).unwrap().into(),
        InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None).unwrap().into(),
        InputOption::new("strict-psr", None, Some(InputOption::VALUE_NONE), "Return a failed status code (1) if PSR-4 or PSR-0 mapping errors are present. Requires --optimize to work.", None).unwrap().into(),
        InputOption::new("strict-ambiguous", None, Some(InputOption::VALUE_NONE), "Return a failed status code (2) if the same class is found in multiple files. Requires --optimize to work.", None).unwrap().into(),
        ]);
        self.set_help(
            "<info>shirabe dump-autoload</info>\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#dump-autoload-dumpautoload",
        );
        Ok(())
    }

    fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer);

        // TODO(plugin): dispatch CommandEvent
        let command_event = CommandEvent::new(
            PluginEvents::COMMAND,
            "dump-autoload",
            input.clone(),
            output,
        );
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None);

        // Clone the Rc<RefCell<Config>> so we can take mutable borrows of composer later
        let config = composer.get_config();

        let mut missing_dependencies = false;
        {
            let local_repo = composer
                .get_repository_manager()
                .borrow()
                .get_local_repository();
            for local_pkg in local_repo.get_canonical_packages()? {
                let install_path = composer
                    .get_installation_manager()
                    .borrow_mut()
                    .get_install_path(local_pkg);
                if install_path.as_deref().is_some_and(|p| !file_exists(p)) {
                    missing_dependencies = true;
                    self.get_io().write("<warning>Not all dependencies are installed. Make sure to run a \"composer install\" to install missing dependencies</warning>");
                    break;
                }
            }
        }

        let optimize = input
            .borrow()
            .get_option("optimize")?
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("optimize-autoloader")
                .as_bool()
                .unwrap_or(false);
        let authoritative = input
            .borrow()
            .get_option("classmap-authoritative")?
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("classmap-authoritative")
                .as_bool()
                .unwrap_or(false);
        let apcu_prefix = input
            .borrow()
            .get_option("apcu-prefix")?
            .as_string()
            .map(|s| s.to_string());
        let apcu = apcu_prefix.is_some()
            || input
                .borrow()
                .get_option("apcu")?
                .as_bool()
                .unwrap_or(false)
            || config
                .borrow_mut()
                .get("apcu-autoloader")
                .as_bool()
                .unwrap_or(false);

        if input
            .borrow()
            .get_option("strict-psr")?
            .as_bool()
            .unwrap_or(false)
            && !optimize
            && !authoritative
        {
            return Err(InvalidArgumentException::new("--strict-psr mode only works with optimized autoloader, use --optimize or --classmap-authoritative if you want a strict return value.".to_string())
            .into());
        }
        if input
            .borrow()
            .get_option("strict-ambiguous")?
            .as_bool()
            .unwrap_or(false)
            && !optimize
            && !authoritative
        {
            return Err(InvalidArgumentException::new("--strict-ambiguous mode only works with optimized autoloader, use --optimize or --classmap-authoritative if you want a strict return value.".to_string())
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

        let platform_requirement_filter = self.get_platform_requirement_filter(input.clone())?;
        if input
            .borrow()
            .get_option("dry-run")?
            .as_bool()
            .unwrap_or(false)
        {
            composer.get_autoload_generator().borrow().set_dry_run(true);
        }
        if input
            .borrow()
            .get_option("no-dev")?
            .as_bool()
            .unwrap_or(false)
        {
            composer
                .get_autoload_generator()
                .borrow()
                .set_dev_mode(false);
        }
        if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
            if input
                .borrow()
                .get_option("no-dev")?
                .as_bool()
                .unwrap_or(false)
            {
                return Err(InvalidArgumentException::new(
                    "You can not use both --no-dev and --dev as they conflict with each other."
                        .to_string(),
                )
                .into());
            }
            composer
                .get_autoload_generator()
                .borrow()
                .set_dev_mode(true);
        }
        composer
            .get_autoload_generator()
            .borrow()
            .set_class_map_authoritative(authoritative);
        composer
            .get_autoload_generator()
            .borrow()
            .set_run_scripts(true);
        composer
            .get_autoload_generator()
            .borrow()
            .set_apcu(apcu, apcu_prefix);
        composer
            .get_autoload_generator()
            .borrow()
            .set_platform_requirement_filter(platform_requirement_filter);
        let strict_ambiguous = input
            .borrow()
            .get_option("strict-ambiguous")?
            .as_bool()
            .unwrap_or(false);

        let local_repo_handle = composer
            .get_repository_manager()
            .borrow()
            .get_local_repository();
        let package = composer.get_package().clone();
        let installation_manager = composer.get_installation_manager();
        let locker = composer.get_locker();
        let autoload_generator = composer.get_autoload_generator();

        let config_ref = config.borrow();
        let class_map = autoload_generator.borrow().dump(
            &config_ref,
            local_repo_handle,
            package,
            installation_manager,
            "composer",
            optimize,
            None,
            Some(locker),
            strict_ambiguous,
        )?;
        let number_of_classes = class_map.map.len();

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
            || (input
                .borrow()
                .get_option("strict-psr")?
                .as_bool()
                .unwrap_or(false)
                && !class_map.get_psr_violations().is_empty())
        {
            return Ok(1);
        }

        if input
            .borrow()
            .get_option("strict-ambiguous")?
            .as_bool()
            .unwrap_or(false)
            && !class_map.get_ambiguous_classes(None)?.is_empty()
        {
            return Ok(2);
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
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for DumpAutoloadCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
