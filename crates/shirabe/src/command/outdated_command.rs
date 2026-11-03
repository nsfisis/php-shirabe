//! ref: composer/src/Composer/Command/OutdatedCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData, CompletionTrait};
use crate::console::input::{InputArgument, InputOption, SuggestedValues};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::impl_php_class;
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::{
    ArrayInput, InputInterface, InputInterfaceByName, InputValue, ParameterName,
};
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct OutdatedCommand {
    base_command_data: BaseCommandData,
}

impl_php_class!(OutdatedCommand, r"Composer\Command\OutdatedCommand");

impl Default for OutdatedCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl OutdatedCommand {
    pub fn new() -> Self {
        let command = OutdatedCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("OutdatedCommand::configure uses static, valid metadata");
        command
    }
}

#[async_trait::async_trait(?Send)]
impl Command for OutdatedCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("outdated".into())?;
        self.set_description("Shows a list of installed packages that have updates available, including their latest version".into());
        self.set_definition(&[
            InputArgument::new5("package", Some(InputArgument::OPTIONAL), "Package to inspect. Or a name including a wildcard (*) to filter lists of packages instead.", None, self.suggest_installed_package(false, false)).unwrap().into(),
        InputOption::new("outdated", Some("o".into()), Some(InputOption::VALUE_NONE), "Show only packages that are outdated (this is the default, but present here for compat with `show`", None).unwrap().into(),
        InputOption::new("all", Some("a".into()), Some(InputOption::VALUE_NONE), "Show all installed packages with their latest versions", None).unwrap().into(),
        InputOption::new("locked", None, Some(InputOption::VALUE_NONE), "Shows updates for packages from the lock file, regardless of what is currently in vendor dir", None).unwrap().into(),
        InputOption::new("direct", Some("D".into()), Some(InputOption::VALUE_NONE), "Shows only packages that are directly required by the root package", None).unwrap().into(),
        InputOption::new("strict", None, Some(InputOption::VALUE_NONE), "Return a non-zero exit code when there are outdated packages", None).unwrap().into(),
        InputOption::new("major-only", Some("M".into()), Some(InputOption::VALUE_NONE), "Show only packages that have major SemVer-compatible updates.", None).unwrap().into(),
        InputOption::new("minor-only", Some("m".into()), Some(InputOption::VALUE_NONE), "Show only packages that have minor SemVer-compatible updates.", None).unwrap().into(),
        InputOption::new("patch-only", Some("p".into()), Some(InputOption::VALUE_NONE), "Show only packages that have patch SemVer-compatible updates.", None).unwrap().into(),
        InputOption::new("sort-by-age", Some("A".into()), Some(InputOption::VALUE_NONE), "Displays the installed version's age, and sorts packages oldest first.", None).unwrap().into(),
        InputOption::new6("format", Some("f".into()), Some(InputOption::VALUE_REQUIRED), "Format of the output: text or json", Some(InputValue::from("text")), SuggestedValues::List(vec![BString::from("json"), BString::from("text")])).unwrap().into(),
        InputOption::new6("ignore", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore specified package(s). Can contain wildcards (*). Use it if you don't want to be informed about new versions of some packages.", None, self.suggest_installed_package(false, false)).unwrap().into(),
        InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables search in require-dev packages.", None).unwrap().into(),
        InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages). Use with the --outdated option", None).unwrap().into(),
        InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages). Use with the --outdated option", None).unwrap().into(),
        ]);
        self.set_help(
            "The outdated command is just a proxy for `shirabe show -l`\n\n\
            The color coding (or signage if you have ANSI colors disabled) for dependency versions is as such:\n\n\
            - <info>green</info> (=): Dependency is in the latest version and is up to date.\n\
            - <comment>yellow</comment> (~): Dependency has a new version available that includes backwards\n  \
              compatibility breaks according to semver, so upgrade when you can but it\n  \
              may involve work.\n\
            - <highlight>red</highlight> (!): Dependency has a new version that is semver-compatible and you should upgrade it.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#outdated".into()
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let mut args: IndexMap<BString, InputValue> = IndexMap::new();
        args.insert(BString::from("command"), InputValue::from("show"));
        args.insert(BString::from("--latest"), InputValue::Bool(true));

        if input
            .borrow()
            .get_option("no-interaction")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--no-interaction"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("no-plugins")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--no-plugins"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("no-scripts")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--no-scripts"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("no-cache")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--no-cache"), InputValue::Bool(true));
        }
        if !input.borrow().get_option("all")?.as_bool().unwrap_or(false) {
            args.insert(BString::from("--outdated"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("direct")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--direct"), InputValue::Bool(true));
        }
        let package_arg = input.borrow().get_argument("package")?;
        if !package_arg.is_null() {
            args.insert(BString::from("package"), package_arg);
        }
        if input
            .borrow()
            .get_option("strict")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--strict"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("major-only")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--major-only"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("minor-only")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--minor-only"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("patch-only")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--patch-only"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("locked")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--locked"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("no-dev")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--no-dev"), InputValue::Bool(true));
        }
        if input
            .borrow()
            .get_option("sort-by-age")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(BString::from("--sort-by-age"), InputValue::Bool(true));
        }
        args.insert(
            BString::from("--ignore-platform-req"),
            input.borrow().get_option("ignore-platform-req")?,
        );
        if input
            .borrow()
            .get_option("ignore-platform-reqs")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert(
                BString::from("--ignore-platform-reqs"),
                InputValue::Bool(true),
            );
        }
        args.insert(
            BString::from("--format"),
            input.borrow().get_option("format")?,
        );
        args.insert(
            BString::from("--ignore"),
            input.borrow().get_option("ignore")?,
        );

        let input = ArrayInput::new(
            args.into_iter()
                .map(|(k, v)| (ParameterName::of(k), v))
                .collect(),
            None,
        )?;

        let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(input));

        let application = {
            let application = self
                .get_application()
                .expect("a proxy command is always attached to its application");
            let application = application.borrow();
            application
                .as_any()
                .downcast_ref::<crate::console::application::Application>()
                .expect("shirabe always installs its own Application")
                .shared()
        };

        Ok(application.run(Some(input), Some(output)).await? as i64)
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
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for OutdatedCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    fn is_proxy_command(&self) -> bool {
        true
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
