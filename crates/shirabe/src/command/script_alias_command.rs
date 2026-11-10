//! ref: composer/src/Composer/Command/ScriptAliasCommand.php sha1:098a85149d8bc3d10182f2f82fad53aaa7c67eb4

use crate::command::{
    BaseCommand, BaseCommandData, base_command_complete, base_command_initialize,
};
use crate::composer::composer_full;
use crate::console::input::{InputArgument, InputOption};
use crate::delegate_base_command_trait_impls_to_inner;
use crate::event_dispatcher::EventFlags;
use crate::util::Platform;
use bstr::{BStr, BString, ByteSlice as _};
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, ToPhpString, bformat, impl_php_class, is_string, php_regex,
    preg_replace_with_limit_and_count,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName};
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct ScriptAliasCommand {
    base_command_data: BaseCommandData,

    script: BString,
    description: BString,
    aliases: Vec<BString>,
}

impl_php_class!(ScriptAliasCommand, r"Composer\Command\ScriptAliasCommand");

impl ScriptAliasCommand {
    pub fn new(
        script: &BStr,
        description: Option<BString>,
        aliases: Vec<BString>,
    ) -> anyhow::Result<Self> {
        let description = description
            .unwrap_or_else(|| bformat!("Runs the {script} script as defined in composer.json"));

        for alias in &aliases {
            if !is_string(&PhpMixed::string(alias.clone())) {
                return Err(InvalidArgumentException::new(BString::from(
                    r#""scripts-aliases" element array values should contain only strings"#,
                ))
                .into());
            }
        }

        let command = Self {
            base_command_data: BaseCommandData::new(None),
            script: script.into(),
            description,
            aliases,
        };
        command.ignore_validation_errors();
        command
            .configure()
            .expect("ScriptAliasCommand::configure uses constructor-provided metadata");
        Ok(command)
    }
}

#[async_trait::async_trait(?Send)]
impl Command for ScriptAliasCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name(self.script.as_bstr())?;
        self.set_description(self.description.as_bstr());
        self.set_aliases(self.aliases.clone())?;
        self.set_definition(&[
            InputOption::new(
                "dev",
                None,
                Some(InputOption::VALUE_NONE),
                "Sets the dev mode.",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "no-dev",
                None,
                Some(InputOption::VALUE_NONE),
                "Disables the dev mode.",
                None,
            )
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
            "The <info>run-script</info> command runs scripts defined in composer.json:\n\n\
            <info>shirabe run-script post-update-cmd</info>\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#run-script-run"
                .into(),
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;
        let dispatcher = composer_full(&composer).get_event_dispatcher().clone();

        let args = input.borrow().get_arguments();

        let dev_mode = input.borrow().get_option("dev")?.as_bool().unwrap_or(false)
            || !input
                .borrow()
                .get_option("no-dev")?
                .as_bool()
                .unwrap_or(false);

        Platform::put_env("COMPOSER_DEV_MODE", if dev_mode { "1" } else { "0" })?;

        let script_alias_input = preg_replace_with_limit_and_count(
            php_regex!(r"{^\S+ ?}"),
            "",
            input.borrow().to_php_string(),
            1,
            None,
        );
        let flags = EventFlags {
            script_alias_input: Some(script_alias_input),
            ..Default::default()
        };

        let args_value: Vec<bstr::BString> = args
            .get(bstr::B("args"))
            .and_then(|v| v.as_array())
            .map(<[bstr::BString]>::to_vec)
            .unwrap_or_default();

        dispatcher
            .borrow_mut()
            .dispatch_script(self.script.as_ref(), dev_mode, args_value, flags)
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
        base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ScriptAliasCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    delegate_base_command_trait_impls_to_inner!(base_command_data);
}
