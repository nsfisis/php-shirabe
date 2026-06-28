//! ref: composer/src/Composer/Command/GlobalCommand.php

use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::console::input::InputArgument;
use crate::factory::Factory;
use crate::util::Filesystem;
use crate::util::Platform;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::ArgvInput;
use shirabe_external_packages::symfony::console::input::ArrayInput;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::input::StringInput;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{LogicException, RuntimeException, chdir};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

#[derive(Debug)]
pub struct GlobalCommand {
    base_command_data: BaseCommandData,
}

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

    // TODO(cli-completion): pub fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions)

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
        } else {
            Err(LogicException {
                message: "Expected an Input instance that is stringable".to_string(),
                code: 0,
            }
            .into())
        }
    }

    fn prepare_subcommand_input(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
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
                return Err(RuntimeException {
                    message: "Could not create home directory".to_string(),
                    code: 0,
                }
                .into());
            }
        }

        chdir(&home).map_err(|_e| RuntimeException {
            message: format!("Could not switch to home directory \"{}\"", home),
            code: 0,
        })?;

        if !quiet {
            self.get_io().borrow().write_error(&format!(
                "<info>Changed current directory to {}</info>",
                home
            ));
        }

        let new_input_str = Preg::replace4(
            r"{\bg(?:l(?:o(?:b(?:a(?:l)?)?)?)?)?\b}",
            "",
            &Self::input_to_string(&*input.borrow())?,
            1,
        );
        // TODO(phase-c): getApplication()->resetComposer() needs the shared shirabe Application
        // handle (deferred with the Application shared-ownership work).

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

    fn run(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let tokens = Preg::split(r"{\s+}", &Self::input_to_string(&*input.borrow())?);
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
        // TODO(phase-c): proxying to Application::run needs the shared shirabe Application handle
        // (deferred with the Application shared-ownership work and command registration).
        let _ = (sub_input, output);
        todo!("global command proxy run pending shared Application handle")
    }

    fn initialize(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for GlobalCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);

    fn is_proxy_command(&self) -> bool {
        true
    }
}
