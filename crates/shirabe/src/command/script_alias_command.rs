//! ref: composer/src/Composer/Command/ScriptAliasCommand.php

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::io::IOInterface;
use crate::util::Platform;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
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

        // PHP also calls parent::__construct() (Symfony Command base) and
        // $this->ignoreValidationErrors().
        // TODO(phase-c): both are Symfony Command base-class operations — the constructor sets up
        // the command's name/definition/application state and ignoreValidationErrors() flips a flag
        // on it. Composer's BaseCommand carries no such Symfony Command state yet (the Symfony
        // Command base is an intentional todo!() stub), so there is nothing to initialize here.
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
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;
        let dispatcher = crate::command::composer_full(&composer)
            .get_event_dispatcher()
            .clone();

        let args = input.borrow().get_arguments();

        // TODO(phase-c): InputInterface has_to_string/get_class_name not modeled in Rust
        // TODO remove for Symfony 6+ as it is then in the interface
        if false {
            return Err(LogicException {
                message: "Expected an Input instance that is stringable".to_string(),
                code: 0,
            }
            .into());
        }

        let dev_mode = input.borrow().get_option("dev")?.as_bool().unwrap_or(false)
            || !input
                .borrow()
                .get_option("no-dev")?
                .as_bool()
                .unwrap_or(false);

        Platform::put_env("COMPOSER_DEV_MODE", if dev_mode { "1" } else { "0" });

        // TODO(phase-c): InputInterface lacks to_string; use a placeholder until it is modeled.
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

        Ok(dispatcher
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
