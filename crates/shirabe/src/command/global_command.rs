//! ref: composer/src/Composer/Command/GlobalCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::console::Application;
use crate::console::input::InputArgument;
use crate::factory::Factory;
use crate::util::Filesystem;
use crate::util::Platform;
use shirabe_pcre::Preg;
use shirabe_php_shim::{
    LogicException, RuntimeException, chdir, impl_php_class, php_regex, preg_split,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::completion::CompletionInput;
use shirabe_symfony_console::completion::{CompletionSuggestions, StringOrSuggestion};
use shirabe_symfony_console::input::ArgvInput;
use shirabe_symfony_console::input::ArrayInput;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::input::StringInput;
use shirabe_symfony_console::output::OutputInterface;
use std::path::Path;

#[derive(Debug)]
pub struct GlobalCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(GlobalCommand, r"Composer\Command\GlobalCommand");

impl Default for GlobalCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalCommand {
    pub fn new() -> Self {
        let command = GlobalCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("GlobalCommand::configure uses static, valid metadata");
        command
    }

    // TODO remove for Symfony 6+ as it is then in the interface.
    // Mirrors PHP's `method_exists($input, '__toString')` guard followed by
    // `$input->__toString()`. `InputInterface` does not declare `__toString`, so the
    // concrete stringable input types are matched explicitly.
    fn input_to_string(input: &dyn InputInterface) -> anyhow::Result<String> {
        let input_any = input.as_any();
        if let Some(argv_input) = input_any.downcast_ref::<ArgvInput>() {
            Ok(argv_input.to_string())
        } else if let Some(array_input) = input_any.downcast_ref::<ArrayInput>() {
            Ok(array_input.to_string())
        } else if let Some(string_input) = input_any.downcast_ref::<StringInput>() {
            Ok(string_input.to_string())
        } else if let Some(completion_input) = input_any.downcast_ref::<CompletionInput>() {
            Ok(completion_input.to_string())
        } else {
            Err(
                LogicException::new("Expected an Input instance that is stringable".to_string())
                    .into(),
            )
        }
    }

    fn prepare_subcommand_input(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        quiet: bool,
    ) -> anyhow::Result<StringInput> {
        if Platform::get_env("COMPOSER").is_some() {
            Platform::clear_env("COMPOSER");
        }

        let config = Factory::create_config(None, None)?;
        let home = config.get("home").as_string().unwrap_or("").to_string();

        if !Path::new(&home).is_dir() {
            let mut fs = Filesystem::new(None);
            fs.ensure_directory_exists(&home)?;
            if !Path::new(&home).is_dir() {
                return Err(
                    RuntimeException::new("Could not create home directory".to_string()).into(),
                );
            }
        }

        chdir(&home).map_err(|e| {
            RuntimeException::with_code_and_previous(
                format!("Could not switch to home directory \"{}\"", home),
                0,
                Some(std::sync::Arc::new(e)),
            )
        })?;

        if !quiet {
            self.get_io().borrow().write_error(&format!(
                "<info>Changed current directory to {}</info>",
                home
            ));
        }

        let new_input_str = Preg::replace4(
            php_regex!(r"{\bg(?:l(?:o(?:b(?:a(?:l)?)?)?)?)?\b}"),
            "",
            &Self::input_to_string(&*input.borrow())?,
            1,
        );
        self.reset_composer()?;

        StringInput::new(&new_input_str)
    }
}

impl Command for GlobalCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("global")?;
        self.set_description("Allows running commands in the global composer dir ($COMPOSER_HOME)");
        self.set_definition(&[
            InputArgument::new("command-name", Some(InputArgument::REQUIRED), "", None)
                .unwrap()
                .into(),
            InputArgument::new(
                "args",
                Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
                "",
                None,
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "Use this command as a wrapper to run other Composer commands\n\
            within the global context of COMPOSER_HOME.\n\n\
            You can use this to install CLI utilities globally, all you need\n\
            is to add the COMPOSER_HOME/vendor/bin dir to your PATH env var.\n\n\
            COMPOSER_HOME is c:\\Users\\<user>\\AppData\\Roaming\\Composer on Windows\n\
            and /home/<user>/.composer on unix systems.\n\n\
            If your system uses freedesktop.org standards, then it will first check\n\
            XDG_CONFIG_HOME or default to /home/<user>/.config/composer\n\n\
            Note: This path may vary depending on customizations to bin-dir in\n\
            composer.json or the environmental variable COMPOSER_BIN_DIR.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#global",
        );
        Ok(())
    }

    fn is_proxy_command(&self) -> bool {
        true
    }

    fn complete(
        &self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        let application = self
            .get_application()
            .expect("a proxy command is always attached to its application");
        if input.must_suggest_argument_values_for("command-name") {
            // The application borrow must be dropped before suggest_values (harmless) and
            // before any command re-entry below.
            let values: Vec<StringOrSuggestion> = {
                let mut app_ref = application.borrow_mut();
                let app = app_ref
                    .as_any_mut()
                    .downcast_mut::<Application>()
                    .expect("shirabe always installs its own Application");
                app.all(None)?
                    .values()
                    // PHP: $command->isHidden() ? null : $command->getName(), then
                    // array_filter drops the nulls.
                    .filter(|command| !command.borrow().is_hidden())
                    .filter_map(|command| command.borrow().get_name())
                    .map(StringOrSuggestion::String)
                    .collect()
            };
            suggestions.suggest_values(values);

            return Ok(());
        }

        let command_name = input.get_argument("command-name")?.to_string();
        let has = {
            let mut app_ref = application.borrow_mut();
            let app = app_ref
                .as_any_mut()
                .downcast_mut::<Application>()
                .expect("shirabe always installs its own Application");
            app.has(&command_name)
        };
        if has {
            let prepared = self.prepare_subcommand_input(
                std::rc::Rc::new(std::cell::RefCell::new(input.clone())),
                true,
            )?;
            let mut input = CompletionInput::from_string(&prepared.to_string(), 2)?;
            let command = {
                let mut app_ref = application.borrow_mut();
                let app = app_ref
                    .as_any_mut()
                    .downcast_mut::<Application>()
                    .expect("shirabe always installs its own Application");
                app.find(&command_name)?
            };
            command.borrow().merge_application_definition(true);

            {
                let command_ref = command.borrow();
                let definition = command_ref.get_definition();
                input.bind(&definition)?;
            }
            command.borrow().complete(&input, suggestions)?;
        }
        Ok(())
    }

    fn run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let tokens = preg_split(
            php_regex!(r"{\s+}"),
            &Self::input_to_string(&*input.borrow())?,
        );
        let mut args: Vec<String> = vec![];
        for token in &tokens {
            if !token.is_empty() && !token.starts_with('-') {
                args.push(token.clone());
                if args.len() >= 2 {
                    break;
                }
            }
        }

        if args.len() < 2 {
            return self.base_run(input, output);
        }

        let sub_input = self.prepare_subcommand_input(input, false)?;
        let sub_input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(sub_input));

        let application = {
            let application = self
                .get_application()
                .expect("a proxy command is always attached to its application");
            let application = application.borrow();
            application
                .as_any()
                .downcast_ref::<Application>()
                .expect("shirabe always installs its own Application")
                .shared()
        };

        Ok(application.run(Some(sub_input), Some(output))? as i64)
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for GlobalCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);

    fn is_proxy_command(&self) -> bool {
        true
    }
}
