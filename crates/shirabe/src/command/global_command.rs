//! ref: composer/src/Composer/Command/GlobalCommand.php

use std::path::Path;

use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::input::ArgvInput;
use shirabe_external_packages::symfony::console::input::ArrayInput;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::input::StringInput;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{AsAny, LogicException, RuntimeException, chdir};

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::factory::Factory;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::util::Filesystem;
use crate::util::Platform;

#[derive(Debug)]
pub struct GlobalCommand {
    base_command_data: BaseCommandData,
}

impl GlobalCommand {
    // TODO(cli-completion): pub fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions)

    pub fn configure(&mut self) {
        self.set_name("global")
            .set_description("Allows running commands in the global composer dir ($COMPOSER_HOME)")
            .set_definition(&[
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
            ])
            .set_help(
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
    }

    // TODO remove for Symfony 6+ as it is then in the interface.
    // Mirrors PHP's `method_exists($input, '__toString')` guard followed by
    // `$input->__toString()`. `InputInterface` does not declare `__toString`, so the
    // concrete stringable input types are matched explicitly.
    fn input_to_string(input: &dyn InputInterface) -> Result<String> {
        let input_any = input.as_any();
        if let Some(argv_input) = input_any.downcast_ref::<ArgvInput>() {
            Ok(argv_input.to_string())
        } else if let Some(array_input) = input_any.downcast_ref::<ArrayInput>() {
            Ok(array_input.to_string())
        } else if input_any.is::<StringInput>() {
            // StringInput's stringification is not exposed by shirabe-external-packages
            // (its inner ArgvInput is pub(crate) and it defines no public __toString).
            todo!("StringInput::__toString is not accessible from the shirabe crate")
        } else {
            Err(LogicException {
                message: "Expected an Input instance that is stringable".to_string(),
                code: 0,
            }
            .into())
        }
    }

    pub fn run(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Result<i64> {
        let tokens = Preg::split(r"{\s+}", &Self::input_to_string(&*input.borrow())?)?;
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
            return self.run(input, output);
        }

        let sub_input = self.prepare_subcommand_input(input, false)?;
        let mut app = self.get_application()?;
        Ok(app.run(
            Some(std::rc::Rc::new(std::cell::RefCell::new(sub_input))),
            Some(output),
        )?)
    }

    fn prepare_subcommand_input(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        quiet: bool,
    ) -> Result<StringInput> {
        if Platform::get_env("COMPOSER").is_some() {
            Platform::clear_env("COMPOSER");
        }

        let mut config = Factory::create_config(None, None)?;
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
            self.get_io().write_error(&format!(
                "<info>Changed current directory to {}</info>",
                home
            ));
        }

        let new_input_str = Preg::replace4(
            r"{\bg(?:l(?:o(?:b(?:a(?:l)?)?)?)?)?\b}",
            "",
            &Self::input_to_string(&*input.borrow())?,
            1,
        )?;
        self.get_application()?.reset_composer();

        Ok(StringInput::new(&new_input_str)?)
    }

    pub fn is_proxy_command(&self) -> bool {
        true
    }
}

impl HasBaseCommandData for GlobalCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
