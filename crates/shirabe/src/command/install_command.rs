//! ref: composer/src/Composer/Command/InstallCommand.php

use anyhow::Result;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::PhpMixed;

use crate::advisory::auditor::Auditor;
use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::installer::Installer;
use crate::io::io_interface::IOInterface;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::util::http_downloader::HttpDownloader;

#[derive(Debug)]
pub struct InstallCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl CompletionTrait for InstallCommand {
    fn require_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Composer {
        todo!()
    }
}

impl InstallCommand {
    pub fn configure(&mut self) {
        let suggest_prefer_install = self.suggest_prefer_install();
        self.inner
            .set_name("install")
            .set_aliases(vec!["i".to_string()])
            .set_description("Installs the project dependencies from the composer.lock file if present, or falls back on the composer.json")
            .set_definition(vec![
                InputOption::new("prefer-source", None, Some(InputOption::VALUE_NONE), "Forces installation from package sources when possible, including VCS information.", None, vec![]),
                InputOption::new("prefer-dist", None, Some(InputOption::VALUE_NONE), "Forces installation from package dist (default behavior).", None, vec![]),
                InputOption::new("prefer-install", None, Some(InputOption::VALUE_REQUIRED), "Forces installation from package dist|source|auto (auto chooses source for dev versions, dist for the rest).", None, suggest_prefer_install),
                InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the operations but will not execute anything (implicitly enables --verbose).", None, vec![]),
                InputOption::new("download-only", None, Some(InputOption::VALUE_NONE), "Download only, do not install packages.", None, vec![]),
                InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "DEPRECATED: Enables installation of require-dev packages (enabled by default, only present for BC).", None, vec![]),
                InputOption::new("no-suggest", None, Some(InputOption::VALUE_NONE), "DEPRECATED: This flag does not exist anymore.", None, vec![]),
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables installation of require-dev packages.", None, vec![]),
                InputOption::new("no-security-blocking", None, Some(InputOption::VALUE_NONE), "Allows installing packages with security advisories or that are abandoned (can also be set via the COMPOSER_NO_SECURITY_BLOCKING=1 env var). Only applies when no lock file is present.", None, vec![]),
                InputOption::new("no-autoloader", None, Some(InputOption::VALUE_NONE), "Skips autoloader generation", None, vec![]),
                InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None, vec![]),
                InputOption::new("no-install", None, Some(InputOption::VALUE_NONE), "Do not use, only defined here to catch misuse of the install command.", None, vec![]),
                InputOption::new("audit", None, Some(InputOption::VALUE_NONE), "Run an audit after installation is complete.", None, vec![]),
                InputOption::new("audit-format", None, Some(InputOption::VALUE_REQUIRED), "Audit output format. Must be \"table\", \"plain\", \"json\", or \"summary\".", Some(PhpMixed::String(Auditor::FORMAT_SUMMARY.to_string())), Auditor::FORMATS.iter().map(|s| s.to_string()).collect()),
                InputOption::new("verbose", Some(PhpMixed::String("v|vv|vvv".to_string())), Some(InputOption::VALUE_NONE), "Shows more details including new commits pulled in when updating packages.", None, vec![]),
                InputOption::new("optimize-autoloader", Some(PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Optimize autoloader during autoloader dump", None, vec![]),
                InputOption::new("classmap-authoritative", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize-autoloader`.", None, vec![]),
                InputOption::new("apcu-autoloader", None, Some(InputOption::VALUE_NONE), "Use APCu to cache found/not-found classes.", None, vec![]),
                InputOption::new("apcu-autoloader-prefix", None, Some(InputOption::VALUE_REQUIRED), "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu-autoloader", None, vec![]),
                InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None, vec![]),
                InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None, vec![]),
                InputArgument::new("packages", Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL), "Should not be provided, use composer require instead to add a given package to composer.json.", None, vec![]),
            ])
            .set_help(
                "The <info>install</info> command reads the composer.lock file from\n\
                the current directory, processes it, and downloads and installs all the\n\
                libraries and dependencies outlined in that file. If the file does not\n\
                exist it will look for composer.json and do the same.\n\n\
                <info>php composer.phar install</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#install-i"
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> Result<i64> {
        let io = self.inner.get_io();

        if input.get_option("dev").as_bool().unwrap_or(false) {
            io.write_error("<warning>You are using the deprecated option \"--dev\". It has no effect and will break in Composer 3.</warning>");
        }
        if input.get_option("no-suggest").as_bool().unwrap_or(false) {
            io.write_error("<warning>You are using the deprecated option \"--no-suggest\". It has no effect and will break in Composer 3.</warning>");
        }

        let args = input.get_argument("packages");
        let args_vec: Vec<String> = args
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if !args_vec.is_empty() {
            io.write_error(&format!(
                "<error>Invalid argument {}. Use \"composer require {}\" instead to add packages to your composer.json.</error>",
                args_vec.join(" "),
                args_vec.join(" ")
            ));
            return Ok(1);
        }

        if input.get_option("no-install").as_bool().unwrap_or(false) {
            io.write_error("<error>Invalid option \"--no-install\". Use \"composer update --no-install\" instead if you are trying to update the composer.lock file.</error>");
            return Ok(1);
        }

        let composer = self.inner.require_composer()?;

        if !composer.get_locker().is_locked() && !HttpDownloader::is_curl_enabled() {
            io.write_error("<warning>Composer is operating significantly slower than normal because you do not have the PHP curl extension enabled.</warning>");
        }

        // TODO(plugin): dispatch CommandEvent
        let command_event = CommandEvent::new(
            PluginEvents::COMMAND.to_string(),
            "install".to_string(),
            Box::new(input),
            Box::new(output),
            vec![],
            vec![],
        );
        composer
            .get_event_dispatcher()
            .dispatch(command_event.get_name(), &command_event);

        let install = Installer::create(io, &composer);

        let config = composer.get_config();
        let (prefer_source, prefer_dist) =
            self.inner.get_preferred_install_options(config, input)?;

        let optimize = input
            .get_option("optimize-autoloader")
            .as_bool()
            .unwrap_or(false)
            || config.get("optimize-autoloader").as_bool().unwrap_or(false);
        let authoritative = input
            .get_option("classmap-authoritative")
            .as_bool()
            .unwrap_or(false)
            || config
                .get("classmap-authoritative")
                .as_bool()
                .unwrap_or(false);
        let apcu_prefix = input
            .get_option("apcu-autoloader-prefix")
            .as_string_opt()
            .map(|s| s.to_string());
        let apcu = apcu_prefix.is_some()
            || input
                .get_option("apcu-autoloader")
                .as_bool()
                .unwrap_or(false)
            || config.get("apcu-autoloader").as_bool().unwrap_or(false);

        composer
            .get_installation_manager()
            .set_output_progress(!input.get_option("no-progress").as_bool().unwrap_or(false));

        install
            .set_dry_run(input.get_option("dry-run").as_bool().unwrap_or(false))
            .set_download_only(input.get_option("download-only").as_bool().unwrap_or(false))
            .set_verbose(input.get_option("verbose").as_bool().unwrap_or(false))
            .set_prefer_source(prefer_source)
            .set_prefer_dist(prefer_dist)
            .set_dev_mode(!input.get_option("no-dev").as_bool().unwrap_or(false))
            .set_dump_autoloader(!input.get_option("no-autoloader").as_bool().unwrap_or(false))
            .set_optimize_autoloader(optimize)
            .set_class_map_authoritative(authoritative)
            .set_apcu_autoloader(apcu, apcu_prefix.as_deref())
            .set_platform_requirement_filter(self.inner.get_platform_requirement_filter(input)?)
            .set_audit_config(
                self.inner
                    .create_audit_config(composer.get_config(), input)?,
            )
            .set_error_on_audit(input.get_option("audit").as_bool().unwrap_or(false));

        if input.get_option("no-plugins").as_bool().unwrap_or(false) {
            install.disable_plugins();
        }

        install.run()
    }
}

impl BaseCommand for InstallCommand {
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
