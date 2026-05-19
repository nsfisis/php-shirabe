//! ref: composer/src/Composer/Command/ScriptAliasCommand.php

use crate::command::base_command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use crate::util::platform::Platform;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{InvalidArgumentException, LogicException, PhpMixed, is_string};

#[derive(Debug)]
pub struct ScriptAliasCommand {
    base_command_data: BaseCommandData,

    script: String,
    description: String,
    aliases: Vec<String>,
}

impl ScriptAliasCommand {
    pub fn new(script: String, description: Option<String>, aliases: Vec<String>) -> Result<Self> {
        let description = description
            .unwrap_or_else(|| format!("Runs the {} script as defined in composer.json", script));

        for alias in &aliases {
            if !is_string(&PhpMixed::String(alias.clone())) {
                return Err(InvalidArgumentException {
                    message:
                        r#""scripts-aliases" element array values should contain only strings"#
                            .to_string(),
                    code: 0,
                }
                .into());
            }
        }

        // TODO(phase-b): BaseCommand::new() / ignore_validation_errors() not yet ported
        Ok(Self {
            base_command_data: BaseCommandData {
                composer: None,
                io: None,
            },
            script,
            description,
            aliases,
        })
    }

    pub fn configure(&mut self) {
        let script = self.script.clone();
        let description = self.description.clone();
        let aliases = self.aliases.clone();
        self.set_name(&script)
            .set_description(&description)
            .set_aliases(&aliases)
            .set_definition(&[
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
            ])
            .set_help(
                "The <info>run-script</info> command runs scripts defined in composer.json:\n\n\
                <info>php composer.phar run-script post-update-cmd</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#run-script-run",
            );
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;

        let args = input.get_arguments();

        // TODO(phase-b): InputInterface has_to_string/get_class_name not modeled in Rust
        // TODO remove for Symfony 6+ as it is then in the interface
        if false {
            return Err(LogicException {
                message: "Expected an Input instance that is stringable".to_string(),
                code: 0,
            }
            .into());
        }

        let dev_mode = input.get_option("dev").as_bool().unwrap_or(false)
            || !input.get_option("no-dev").as_bool().unwrap_or(false);

        Platform::put_env("COMPOSER_DEV_MODE", if dev_mode { "1" } else { "0" });

        // TODO(phase-b): InputInterface lacks to_string; use a placeholder
        let input_as_string = String::new();
        let _ = input;
        let script_alias_input = Preg::replace4(r"{^\S+ ?}", "", &input_as_string, 1)?;
        let mut flags = indexmap::IndexMap::new();
        flags.insert(
            "script-alias-input".to_string(),
            PhpMixed::String(script_alias_input),
        );

        let args_value: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_list())
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch_script(&self.script, dev_mode, args_value, flags)?)
    }
}

impl HasBaseCommandData for ScriptAliasCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
