//! ref: composer/src/Composer/Command/OutdatedCommand.php

use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::io::io_interface::IOInterface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct OutdatedCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl CompletionTrait for OutdatedCommand {}

impl OutdatedCommand {
    pub fn configure(&mut self) {
        let suggest_installed_package = self.suggest_installed_package(false, false);
        let suggest_installed_package_for_ignore = self.suggest_installed_package(false, false);
        self.inner
            .set_name("outdated")
            .set_description("Shows a list of installed packages that have updates available, including their latest version")
            .set_definition(vec![
                InputArgument::new("package", Some(InputArgument::OPTIONAL), "Package to inspect. Or a name including a wildcard (*) to filter lists of packages instead.", None, suggest_installed_package),
                InputOption::new("outdated", Some(PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Show only packages that are outdated (this is the default, but present here for compat with `show`", None, vec![]),
                InputOption::new("all", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Show all installed packages with their latest versions", None, vec![]),
                InputOption::new("locked", None, Some(InputOption::VALUE_NONE), "Shows updates for packages from the lock file, regardless of what is currently in vendor dir", None, vec![]),
                InputOption::new("direct", Some(PhpMixed::String("D".to_string())), Some(InputOption::VALUE_NONE), "Shows only packages that are directly required by the root package", None, vec![]),
                InputOption::new("strict", None, Some(InputOption::VALUE_NONE), "Return a non-zero exit code when there are outdated packages", None, vec![]),
                InputOption::new("major-only", Some(PhpMixed::String("M".to_string())), Some(InputOption::VALUE_NONE), "Show only packages that have major SemVer-compatible updates.", None, vec![]),
                InputOption::new("minor-only", Some(PhpMixed::String("m".to_string())), Some(InputOption::VALUE_NONE), "Show only packages that have minor SemVer-compatible updates.", None, vec![]),
                InputOption::new("patch-only", Some(PhpMixed::String("p".to_string())), Some(InputOption::VALUE_NONE), "Show only packages that have patch SemVer-compatible updates.", None, vec![]),
                InputOption::new("sort-by-age", Some(PhpMixed::String("A".to_string())), Some(InputOption::VALUE_NONE), "Displays the installed version's age, and sorts packages oldest first.", None, vec![]),
                InputOption::new("format", Some(PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "Format of the output: text or json", Some(PhpMixed::String("text".to_string())), vec!["json".to_string(), "text".to_string()]),
                InputOption::new("ignore", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore specified package(s). Can contain wildcards (*). Use it if you don't want to be informed about new versions of some packages.", None, suggest_installed_package_for_ignore),
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables search in require-dev packages.", None, vec![]),
                InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages). Use with the --outdated option", None, vec![]),
                InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages). Use with the --outdated option", None, vec![]),
            ])
            .set_help(
                "The outdated command is just a proxy for `composer show -l`\n\n\
                The color coding (or signage if you have ANSI colors disabled) for dependency versions is as such:\n\n\
                - <info>green</info> (=): Dependency is in the latest version and is up to date.\n\
                - <comment>yellow</comment> (~): Dependency has a new version available that includes backwards\n  \
                  compatibility breaks according to semver, so upgrade when you can but it\n  \
                  may involve work.\n\
                - <highlight>red</highlight> (!): Dependency has a new version that is semver-compatible and you should upgrade it.\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#outdated"
            );
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> Result<i64> {
        let mut args: IndexMap<String, PhpMixed> = IndexMap::new();
        args.insert("command".to_string(), PhpMixed::String("show".to_string()));
        args.insert("--latest".to_string(), PhpMixed::Bool(true));

        if input
            .get_option("no-interaction")
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--no-interaction".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("no-plugins").as_bool().unwrap_or(false) {
            args.insert("--no-plugins".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("no-scripts").as_bool().unwrap_or(false) {
            args.insert("--no-scripts".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("no-cache").as_bool().unwrap_or(false) {
            args.insert("--no-cache".to_string(), PhpMixed::Bool(true));
        }
        if !input.get_option("all").as_bool().unwrap_or(false) {
            args.insert("--outdated".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("direct").as_bool().unwrap_or(false) {
            args.insert("--direct".to_string(), PhpMixed::Bool(true));
        }
        let package_arg = input.get_argument("package");
        if !matches!(package_arg, PhpMixed::Null) {
            args.insert("package".to_string(), package_arg);
        }
        if input.get_option("strict").as_bool().unwrap_or(false) {
            args.insert("--strict".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("major-only").as_bool().unwrap_or(false) {
            args.insert("--major-only".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("minor-only").as_bool().unwrap_or(false) {
            args.insert("--minor-only".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("patch-only").as_bool().unwrap_or(false) {
            args.insert("--patch-only".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("locked").as_bool().unwrap_or(false) {
            args.insert("--locked".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("no-dev").as_bool().unwrap_or(false) {
            args.insert("--no-dev".to_string(), PhpMixed::Bool(true));
        }
        if input.get_option("sort-by-age").as_bool().unwrap_or(false) {
            args.insert("--sort-by-age".to_string(), PhpMixed::Bool(true));
        }
        args.insert(
            "--ignore-platform-req".to_string(),
            input.get_option("ignore-platform-req"),
        );
        if input
            .get_option("ignore-platform-reqs")
            .as_bool()
            .unwrap_or(false)
        {
            args.insert("--ignore-platform-reqs".to_string(), PhpMixed::Bool(true));
        }
        args.insert("--format".to_string(), input.get_option("format"));
        args.insert("--ignore".to_string(), input.get_option("ignore"));

        let input = ArrayInput::new(args);

        self.inner.get_application().run(&input, output)
    }

    pub fn is_proxy_command(&self) -> bool {
        true
    }
}

impl BaseCommand for OutdatedCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.io
    }
}
