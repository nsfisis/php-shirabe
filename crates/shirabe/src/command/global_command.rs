//! ref: composer/src/Composer/Command/GlobalCommand.php sha1:9bf70d11a1ca3b7779ab1c6b71b74f0a4a009fe0

use crate::command::{BaseCommand, BaseCommandData, base_command_initialize};
use crate::console::Application;
use crate::console::input::InputArgument;
use crate::delegate_base_command_trait_impls_to_inner;
use crate::factory::Factory;
use crate::io::IOInterfaceWrite;
use crate::util::{Filesystem, Platform};
use bstr::BString;
use shirabe_php_shim::{
    RuntimeException, ToPhpString, bformat, boolval, chdir, impl_php_class, path_from_bytes,
    php_regex, preg_replace_with_limit_and_count, preg_split,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::completion::{
    CompletionInput, CompletionSuggestions, StringOrSuggestion,
};
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName, StringInput};
use shirabe_symfony_console::output::OutputInterface;

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

    fn prepare_subcommand_input(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        quiet: bool,
    ) -> anyhow::Result<StringInput> {
        if boolval(&Platform::get_env("COMPOSER")) {
            Platform::clear_env("COMPOSER")?;
        }

        let config = Factory::create_config(None, None)?;
        let home = config.get_home()?;
        let home = path_from_bytes(&home);

        if !home.is_dir() {
            let mut fs = Filesystem::new(None);
            fs.ensure_directory_exists(home)?;
            if !home.is_dir() {
                return Err(RuntimeException::new(BString::from(
                    "Could not create home directory",
                ))
                .into());
            }
        }

        chdir(home).map_err(|e| {
            RuntimeException::with_code_and_previous(
                bformat!("Could not switch to home directory \"{home}\""),
                0,
                Some(std::sync::Arc::new(e)),
            )
        })?;

        if !quiet {
            self.get_io()
                .borrow()
                .write_error(bformat!("<info>Changed current directory to {home}</info>"));
        }

        let new_input_str = preg_replace_with_limit_and_count(
            php_regex!(r"{\bg(?:l(?:o(?:b(?:a(?:l)?)?)?)?)?\b}"),
            "",
            input.borrow().to_php_string(),
            1,
            None,
        );
        self.reset_composer()?;

        StringInput::new(&new_input_str)
    }
}

#[async_trait::async_trait(?Send)]
impl Command for GlobalCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("global".into())?;
        self.set_description(
            "Allows running commands in the global composer dir ($COMPOSER_HOME)".into(),
        );
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
            Read more at https://getcomposer.org/doc/03-cli.md#global"
                .into(),
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

        let command_name = input.get_argument("command-name")?.to_php_string();
        let has = {
            let mut app_ref = application.borrow_mut();
            let app = app_ref
                .as_any_mut()
                .downcast_mut::<Application>()
                .expect("shirabe always installs its own Application");
            app.has(command_name.as_ref())
        };
        if has {
            let prepared = self.prepare_subcommand_input(
                std::rc::Rc::new(std::cell::RefCell::new(input.clone())),
                true,
            )?;
            let mut input = CompletionInput::from_string(prepared.to_php_string(), 2)?;
            let command = {
                let mut app_ref = application.borrow_mut();
                let app = app_ref
                    .as_any_mut()
                    .downcast_mut::<Application>()
                    .expect("shirabe always installs its own Application");
                app.find(command_name.as_ref())?
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

    async fn run(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let tokens = preg_split(php_regex!(r"{\s+}"), input.borrow().to_php_string());
        let mut args: Vec<&bstr::BString> = vec![];
        for token in &tokens {
            if !token.is_empty() && !token.starts_with(b"-") {
                args.push(token);
                if args.len() >= 2 {
                    break;
                }
            }
        }

        if args.len() < 2 {
            return self.base_run(input, output).await;
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

        Ok(application.run(Some(sub_input), Some(output)).await? as i64)
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
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    delegate_base_command_trait_impls_to_inner!(base_command_data);

    fn is_proxy_command(&self) -> bool {
        true
    }
}
