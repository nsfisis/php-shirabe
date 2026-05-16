//! ref: composer/src/Composer/Command/RunScriptCommand.php

use anyhow::Result;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, RuntimeException};

use crate::command::base_command::BaseCommand;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::script::event::ScriptEvent;
use crate::script::script_events::ScriptEvents;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct RunScriptCommand {
    inner: BaseCommand,
    script_events: Vec<&'static str>,
}

impl RunScriptCommand {
    pub fn new() -> Self {
        Self {
            inner: BaseCommand::new(),
            script_events: vec![
                ScriptEvents::PRE_INSTALL_CMD,
                ScriptEvents::POST_INSTALL_CMD,
                ScriptEvents::PRE_UPDATE_CMD,
                ScriptEvents::POST_UPDATE_CMD,
                ScriptEvents::PRE_STATUS_CMD,
                ScriptEvents::POST_STATUS_CMD,
                ScriptEvents::POST_ROOT_PACKAGE_INSTALL,
                ScriptEvents::POST_CREATE_PROJECT_CMD,
                ScriptEvents::PRE_ARCHIVE_CMD,
                ScriptEvents::POST_ARCHIVE_CMD,
                ScriptEvents::PRE_AUTOLOAD_DUMP,
                ScriptEvents::POST_AUTOLOAD_DUMP,
            ],
        }
    }

    pub fn configure(&mut self) {
        self.inner
            .set_name("run-script")
            .set_aliases(vec!["run".to_string()])
            .set_description("Runs the scripts defined in composer.json")
            .set_definition(vec![
                // completion callback (runtime script names) is deferred to Phase B
                InputArgument::new(
                    "script",
                    Some(InputArgument::OPTIONAL),
                    "Script name to run.",
                    None,
                    vec![],
                ),
                InputArgument::new(
                    "args",
                    Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL),
                    "",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "timeout",
                    None,
                    Some(InputOption::VALUE_REQUIRED),
                    "Sets script timeout in seconds, or 0 for never.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "dev",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Sets the dev mode.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-dev",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Disables the dev mode.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "list",
                    Some(PhpMixed::String("l".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "List scripts.",
                    None,
                    vec![],
                ),
            ])
            .set_help(
                "The <info>run-script</info> command runs scripts defined in composer.json:\n\n\
                <info>php composer.phar run-script post-update-cmd</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#run-script-run",
            );
    }

    pub fn interact(
        &self,
        input: &mut dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<()> {
        let scripts = self.get_scripts()?;
        if scripts.is_empty() {
            return Ok(());
        }

        if input.get_argument("script").as_string_opt().is_some()
            || input.get_option("list").as_bool().unwrap_or(false)
        {
            return Ok(());
        }

        let mut options = indexmap::IndexMap::new();
        for script in &scripts {
            options.insert(script.0.clone(), script.1.clone());
        }

        let io = self.inner.get_io();
        let script = io.select(
            "Script to run: ".to_string(),
            options.keys().cloned().collect(),
            PhpMixed::String(String::new()),
            PhpMixed::Int(1),
            "Invalid script name \"%s\"".to_string(),
            false,
        );

        if let Some(selected) = script.as_string() {
            input.set_argument("script", selected);
        }

        Ok(())
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> Result<i64> {
        if input.get_option("list").as_bool().unwrap_or(false) {
            return self.list_scripts(output);
        }

        let script = match input.get_argument("script").as_string_opt() {
            None => {
                return Err(RuntimeException {
                    message: "Missing required argument \"script\"".to_string(),
                    code: 0,
                }
                .into());
            }
            Some(s) => s.to_string(),
        };

        if !self.script_events.contains(&script.as_str()) {
            let const_name = script.to_uppercase().replace('-', "_");
            if ScriptEvents::is_defined(&const_name) {
                return Err(InvalidArgumentException {
                    message: format!("Script \"{}\" cannot be run with this command", script),
                    code: 0,
                }
                .into());
            }
        }

        let composer = self.inner.require_composer()?;
        let dev_mode = input.get_option("dev").as_bool().unwrap_or(false)
            || !input.get_option("no-dev").as_bool().unwrap_or(false);
        let event = ScriptEvent::new(script.clone(), &composer, self.inner.get_io(), dev_mode);
        let has_listeners = composer.get_event_dispatcher().has_event_listeners(&event);
        if !has_listeners {
            return Err(InvalidArgumentException {
                message: format!("Script \"{}\" is not defined in this package", script),
                code: 0,
            }
            .into());
        }

        let args: Vec<String> = input
            .get_argument("args")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(timeout_val) = input.get_option("timeout").as_string_opt() {
            let timeout_str = timeout_val.to_string();
            if !timeout_str.chars().all(|c| c.is_ascii_digit()) {
                return Err(RuntimeException {
                    message:
                        "Timeout value must be numeric and positive if defined, or 0 for forever"
                            .to_string(),
                    code: 0,
                }
                .into());
            }
            let timeout: i64 = timeout_str.parse().unwrap_or(0);
            ProcessExecutor::set_timeout(timeout);
        }

        Platform::put_env("COMPOSER_DEV_MODE", if dev_mode { "1" } else { "0" });

        Ok(composer
            .get_event_dispatcher()
            .dispatch_script(&script, dev_mode, args)?)
    }

    fn list_scripts(&self, output: &dyn OutputInterface) -> Result<i64> {
        let scripts = self.get_scripts()?;
        if scripts.is_empty() {
            return Ok(0);
        }

        let io = self.inner.get_io();
        io.write_error("<info>scripts:</info>");
        let table: Vec<Vec<String>> = scripts
            .iter()
            .map(|(name, desc)| vec![format!("  {}", name), desc.clone()])
            .collect();

        self.inner.render_table(table, output);

        Ok(0)
    }

    fn get_scripts(&self) -> Result<Vec<(String, String)>> {
        let scripts = self.inner.require_composer()?.get_package().get_scripts();
        if scripts.is_empty() {
            return Ok(vec![]);
        }

        let mut result: Vec<(String, String)> = vec![];
        for (name, _script) in scripts {
            let description = self
                .inner
                .get_application()
                .find(&name)
                .map(|cmd| cmd.get_description().unwrap_or("").to_string())
                .unwrap_or_default();
            result.push((name, description));
        }

        Ok(result)
    }
}
