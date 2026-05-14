//! ref: composer/src/Composer/Command/ScriptAliasCommand.php

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{is_string, InvalidArgumentException, LogicException, PhpMixed};
use crate::command::base_command::BaseCommand;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::util::platform::Platform;

pub struct ScriptAliasCommand {
    inner: BaseCommand,
    script: String,
    description: String,
    aliases: Vec<String>,
}

impl ScriptAliasCommand {
    pub fn new(script: String, description: Option<String>, aliases: Vec<String>) -> Result<Self> {
        let description = description.unwrap_or_else(|| format!("Runs the {} script as defined in composer.json", script));

        for alias in &aliases {
            if !is_string(&PhpMixed::String(alias.clone())) {
                return Err(InvalidArgumentException {
                    message: r#""scripts-aliases" element array values should contain only strings"#.to_string(),
                    code: 0,
                }.into());
            }
        }

        let mut inner = BaseCommand::new();
        inner.ignore_validation_errors();

        Ok(Self { inner, script, description, aliases })
    }

    pub fn configure(&mut self) {
        self.inner
            .set_name(&self.script)
            .set_description(&self.description)
            .set_aliases(self.aliases.clone())
            .set_definition(vec![
                InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "Sets the dev mode.", None, vec![]),
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables the dev mode.", None, vec![]),
                InputArgument::new("args", Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL), "", None, vec![]),
            ])
            .set_help(
                "The <info>run-script</info> command runs scripts defined in composer.json:\n\n\
                <info>php composer.phar run-script post-update-cmd</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#run-script-run"
            );
    }

    pub fn execute(&mut self, input: &dyn InputInterface, _output: &dyn OutputInterface) -> Result<i64> {
        let composer = self.inner.require_composer()?;

        let args = input.get_arguments();

        // TODO remove for Symfony 6+ as it is then in the interface
        if !input.has_to_string() {
            return Err(LogicException {
                message: format!("Expected an Input instance that is stringable, got {}", input.get_class_name()),
                code: 0,
            }.into());
        }

        let dev_mode = input.get_option("dev").as_bool().unwrap_or(false)
            || !input.get_option("no-dev").as_bool().unwrap_or(false);

        Platform::put_env("COMPOSER_DEV_MODE", if dev_mode { "1" } else { "0" });

        let script_alias_input = Preg::replace_limit(r"^\S+ ?", "", &input.to_string(), 1);
        let mut flags = indexmap::IndexMap::new();
        flags.insert("script-alias-input".to_string(), PhpMixed::String(script_alias_input));

        let args_value = args.get("args").cloned().unwrap_or(PhpMixed::Null);

        Ok(composer.get_event_dispatcher().dispatch_script(&self.script, dev_mode, args_value, flags)?)
    }
}
