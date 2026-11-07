//! ref: composer/src/Composer/Command/RunScriptCommand.php sha1:fd39fdf758c5f1fd032eefcb82e4ce49beba2882

use crate::command::{
    BaseCommand, BaseCommandData, base_command_complete, base_command_initialize,
};
use crate::composer::composer_full;
use crate::console::input::{InputArgument, InputOption, SuggestedValues};
use crate::delegate_base_command_trait_impls_to_inner;
use crate::event_dispatcher::EventFlags;
use crate::io::IOInterfaceWrite;
use crate::script::{Event as ScriptEvent, ScriptEvents};
use crate::util::{Platform, ProcessExecutor};
use bstr::{BStr, BString};
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, RuntimeException, bformat, impl_php_class, str_replace,
    strtoupper, utf8_text,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::exception::{CommandNotFoundException, NamespaceNotFoundException};
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName, InputValue};
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct RunScriptCommand {
    base_command_data: BaseCommandData,

    script_events: Vec<&'static BStr>,
}

impl_php_class!(RunScriptCommand, r"Composer\Command\RunScriptCommand");

impl Default for RunScriptCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl RunScriptCommand {
    pub fn new() -> Self {
        let command = RunScriptCommand {
            base_command_data: BaseCommandData::new(None),
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
        };
        command
            .configure()
            .expect("RunScriptCommand::configure uses static, valid metadata");
        command
    }

    fn list_scripts(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let scripts = self.get_scripts()?;
        if scripts.is_empty() {
            return Ok(0);
        }

        let io = self.get_io();
        io.write_error("<info>scripts:</info>");
        let table: Vec<Vec<BString>> = scripts
            .iter()
            .map(|(name, desc)| vec![bformat!("  {name}"), desc.clone()])
            .collect();

        self.render_table(table, output)?;

        Ok(0)
    }

    fn get_scripts(&self) -> anyhow::Result<Vec<(BString, BString)>> {
        let composer = self.require_composer(None, None)?;
        let scripts = composer_full(&composer).get_package().get_scripts();
        drop(composer);
        if scripts.is_empty() {
            return Ok(vec![]);
        }

        let mut result: Vec<(BString, BString)> = vec![];
        for (name, _script) in scripts {
            let mut description = BString::default();
            if let Some(application) = self.get_application() {
                match application.borrow_mut().find(name.as_ref()) {
                    Ok(cmd) => description = cmd.borrow().get_description(),
                    Err(e)
                        if e.is_instanceof::<CommandNotFoundException>()
                            || e.is_instanceof::<NamespaceNotFoundException>() => {}
                    Err(e) => return Err(e),
                }
            }
            result.push((name, description));
        }

        Ok(result)
    }
}

#[async_trait::async_trait(?Send)]
impl Command for RunScriptCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("run-script".into())?;
        self.set_aliases(vec!["run".into()])?;
        self.set_description("Runs the scripts defined in composer.json".into());
        self.set_definition(&[
            // PHP passes an inline closure here (it takes no arguments; PHP tolerates the
            // extra ones the caller passes).
            InputArgument::with_suggested_values(
                "script",
                Some(InputArgument::OPTIONAL),
                "Script name to run.",
                None,
                SuggestedValues::Closure(Box::new(|this, _input, _suggestions| {
                    let this = this
                        .as_any()
                        .downcast_ref::<RunScriptCommand>()
                        .expect("the script suggestions are bound to RunScriptCommand");
                    // PHP: array_map(fn ($script) => $script['name'], $this->getScripts())
                    Ok(this
                        .get_scripts()?
                        .into_iter()
                        .map(|(name, _description)| name)
                        .collect())
                })),
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
            InputOption::new(
                "timeout",
                None,
                Some(InputOption::VALUE_REQUIRED),
                "Sets script timeout in seconds, or 0 for never.",
                None,
            )
            .unwrap()
            .into(),
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
            InputOption::new(
                "list",
                Some("l".into()),
                Some(InputOption::VALUE_NONE),
                "List scripts.",
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

    fn interact(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        let _ = (|| -> anyhow::Result<()> {
            let scripts = self.get_scripts()?;
            if scripts.is_empty() {
                return Ok(());
            }

            if input.borrow().get_argument("script")?.as_string().is_some()
                || input
                    .borrow()
                    .get_option("list")?
                    .as_bool()
                    .unwrap_or(false)
            {
                return Ok(());
            }

            let mut options = indexmap::IndexMap::new();
            for script in &scripts {
                options.insert(script.0.clone(), script.1.clone());
            }

            let io = self.get_io();
            let script = io.select(
                "Script to run: ",
                options,
                InputValue::from(""),
                Some(1),
                "Invalid script name \"%s\"",
                false,
            )?;

            input.borrow_mut().set_argument("script", script)?;

            Ok(())
        })();
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        if input
            .borrow()
            .get_option("list")?
            .as_bool()
            .unwrap_or(false)
        {
            return self.list_scripts(output);
        }

        let script = match input.borrow().get_argument("script")?.as_string() {
            None => {
                return Err(RuntimeException::new(BString::from(
                    "Missing required argument \"script\"",
                ))
                .into());
            }
            Some(s) => s.to_owned(),
        };

        if !self.script_events.iter().any(|event| script == *event) {
            let const_name = str_replace("-", "_", strtoupper(&script));
            if ScriptEvents::is_defined(&const_name) {
                return Err(InvalidArgumentException::new(bformat!(
                    "Script \"{script}\" cannot be run with this command"
                ))
                .into());
            }
        }

        let composer = self.require_composer(None, None)?;
        let dispatcher = composer_full(&composer).get_event_dispatcher().clone();
        let dev_mode = input.borrow().get_option("dev")?.as_bool().unwrap_or(false)
            || !input
                .borrow()
                .get_option("no-dev")?
                .as_bool()
                .unwrap_or(false);
        let io = self.get_io();
        let event = ScriptEvent::new(
            script.clone(),
            composer
                .as_full()
                .expect("require_composer returns a full Composer")
                .downgrade(),
            io,
            dev_mode,
            vec![],
            EventFlags::default(),
        );
        let has_listeners = dispatcher.borrow_mut().has_event_listeners(&event);
        if !has_listeners {
            return Err(InvalidArgumentException::new(bformat!(
                "Script \"{script}\" is not defined in this package"
            ))
            .into());
        }

        let args: Vec<bstr::BString> = input
            .borrow()
            .get_argument("args")?
            .as_array()
            .map(<[bstr::BString]>::to_vec)
            .unwrap_or_default();

        if let Some(timeout_val) = input.borrow().get_option("timeout")?.as_string() {
            if !timeout_val.iter().all(u8::is_ascii_digit) {
                return Err(RuntimeException::new(BString::from(
                    "Timeout value must be numeric and positive if defined, or 0 for forever",
                ))
                .into());
            }
            let timeout: i64 = utf8_text(timeout_val)
                .and_then(|text| text.parse().ok())
                .unwrap_or(0);
            ProcessExecutor::set_timeout(timeout);
        }

        Platform::put_env("COMPOSER_DEV_MODE", if dev_mode { "1" } else { "0" })?;

        dispatcher.borrow_mut().dispatch_script(
            script.as_ref(),
            dev_mode,
            args,
            EventFlags::default(),
        )
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

impl BaseCommand for RunScriptCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    delegate_base_command_trait_impls_to_inner!(base_command_data);
}
