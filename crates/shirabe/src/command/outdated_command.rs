//! ref: composer/src/Composer/Command/OutdatedCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::ArrayInput;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

use crate::advisory::AuditConfig;
use crate::command::BaseCommand;
use crate::command::BaseCommandData;
use crate::command::base_command::base_command_initialize;
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;

#[derive(Debug)]
pub struct OutdatedCommand {
    base_command_data: BaseCommandData,
}

impl Default for OutdatedCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl OutdatedCommand {
    pub fn new() -> Self {
        let mut command = OutdatedCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("OutdatedCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for OutdatedCommand {
    fn configure(&mut self) -> anyhow::Result<()> {
        // TODO(cli-completion): suggest_installed_package(false, false) for `package` argument and `--ignore` option
        self.set_name("outdated")?;
        self.set_description("Shows a list of installed packages that have updates available, including their latest version");
        self.set_definition(&[
            InputArgument::new("package", Some(InputArgument::OPTIONAL), "Package to inspect. Or a name including a wildcard (*) to filter lists of packages instead.", None).unwrap().into(),
        InputOption::new("outdated", Some(PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Show only packages that are outdated (this is the default, but present here for compat with `show`", None).unwrap().into(),
        InputOption::new("all", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Show all installed packages with their latest versions", None).unwrap().into(),
        InputOption::new("locked", None, Some(InputOption::VALUE_NONE), "Shows updates for packages from the lock file, regardless of what is currently in vendor dir", None).unwrap().into(),
        InputOption::new("direct", Some(PhpMixed::String("D".to_string())), Some(InputOption::VALUE_NONE), "Shows only packages that are directly required by the root package", None).unwrap().into(),
        InputOption::new("strict", None, Some(InputOption::VALUE_NONE), "Return a non-zero exit code when there are outdated packages", None).unwrap().into(),
        InputOption::new("major-only", Some(PhpMixed::String("M".to_string())), Some(InputOption::VALUE_NONE), "Show only packages that have major SemVer-compatible updates.", None).unwrap().into(),
        InputOption::new("minor-only", Some(PhpMixed::String("m".to_string())), Some(InputOption::VALUE_NONE), "Show only packages that have minor SemVer-compatible updates.", None).unwrap().into(),
        InputOption::new("patch-only", Some(PhpMixed::String("p".to_string())), Some(InputOption::VALUE_NONE), "Show only packages that have patch SemVer-compatible updates.", None).unwrap().into(),
        InputOption::new("sort-by-age", Some(PhpMixed::String("A".to_string())), Some(InputOption::VALUE_NONE), "Displays the installed version's age, and sorts packages oldest first.", None).unwrap().into(),
        InputOption::new("format", Some(PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Format of the output: text or json", Some(PhpMixed::String("text".to_string()))).unwrap().into(),
        InputOption::new("ignore", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore specified package(s). Can contain wildcards (*). Use it if you don't want to be informed about new versions of some packages.", None).unwrap().into(),
        InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables search in require-dev packages.", None).unwrap().into(),
        InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages). Use with the --outdated option", None).unwrap().into(),
        InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages). Use with the --outdated option", None).unwrap().into(),
        ]);
        self.set_help(
            "The outdated command is just a proxy for `composer show -l`\n\n\
            The color coding (or signage if you have ANSI colors disabled) for dependency versions is as such:\n\n\
            - <info>green</info> (=): Dependency is in the latest version and is up to date.\n\
            - <comment>yellow</comment> (~): Dependency has a new version available that includes backwards\n  \
              compatibility breaks according to semver, so upgrade when you can but it\n  \
              may involve work.\n\
            - <highlight>red</highlight> (!): Dependency has a new version that is semver-compatible and you should upgrade it.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#outdated"
        );
        Ok(())
    }

    fn execute(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let mut args: IndexMap<String, PhpMixed> = IndexMap::new();
        args.insert("command".to_string(), PhpMixed::String("show".to_string()));
        args.insert("--latest".to_string(), PhpMixed::Bool(true));

        if input
            .borrow()
            .get_option("no-interaction")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--no-interaction".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("no-plugins")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--no-plugins".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("no-scripts")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--no-scripts".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("no-cache")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--no-cache".to_string(), PhpMixed::Bool(true));
        }
        if !input.borrow().get_option("all")?.as_bool().unwrap_or(false) {
            args.insert("--outdated".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("direct")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--direct".to_string(), PhpMixed::Bool(true));
        }
        let package_arg = input.borrow().get_argument("package")?;
        if !matches!(package_arg, PhpMixed::Null) {
            args.insert("package".to_string(), package_arg);
        }
        if input
            .borrow()
            .get_option("strict")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--strict".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("major-only")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--major-only".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("minor-only")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--minor-only".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("patch-only")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--patch-only".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("locked")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--locked".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("no-dev")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--no-dev".to_string(), PhpMixed::Bool(true));
        }
        if input
            .borrow()
            .get_option("sort-by-age")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--sort-by-age".to_string(), PhpMixed::Bool(true));
        }
        args.insert(
            "--ignore-platform-req".to_string(),
            input.borrow().get_option("ignore-platform-req")?,
        );
        if input
            .borrow()
            .get_option("ignore-platform-reqs")?
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--ignore-platform-reqs".to_string(), PhpMixed::Bool(true));
        }
        args.insert("--format".to_string(), input.borrow().get_option("format")?);
        args.insert("--ignore".to_string(), input.borrow().get_option("ignore")?);

        let input = ArrayInput::new(
            args.into_iter()
                .map(|(k, v)| (PhpMixed::String(k), v))
                .collect(),
            None,
        )?;

        let input: Rc<RefCell<dyn InputInterface>> = Rc::new(RefCell::new(input));
        // TODO(phase-c): proxying to ShowCommand via Application::run needs the shared shirabe
        // Application handle (deferred with the Application shared-ownership work and registration).
        let _ = (input, output);
        todo!("outdated command proxy run pending shared Application handle")
    }

    fn initialize(
        &mut self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for OutdatedCommand {
    fn command_data_mut(
        &mut self,
    ) -> &mut shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data_mut()
    }

    fn is_proxy_command(&self) -> bool {
        true
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}
