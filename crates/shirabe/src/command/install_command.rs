//! ref: composer/src/Composer/Command/InstallCommand.php

use anyhow::Result;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::PhpMixed;

use crate::advisory::Auditor;
use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::installer::Installer;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::util::HttpDownloader;

#[derive(Debug)]
pub struct InstallCommand {
    base_command_data: BaseCommandData,
}

impl InstallCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_prefer_install() for `prefer-install` option
        self
            .set_name("install")
            .set_aliases(&["i".to_string()])
            .set_description("Installs the project dependencies from the composer.lock file if present, or falls back on the composer.json")
            .set_definition(&[
                InputOption::new("prefer-source", None, Some(InputOption::VALUE_NONE), "Forces installation from package sources when possible, including VCS information.", None).unwrap().into(),
                InputOption::new("prefer-dist", None, Some(InputOption::VALUE_NONE), "Forces installation from package dist (default behavior).", None).unwrap().into(),
                InputOption::new("prefer-install", None, Some(InputOption::VALUE_REQUIRED), "Forces installation from package dist|source|auto (auto chooses source for dev versions, dist for the rest).", None).unwrap().into(),
                InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the operations but will not execute anything (implicitly enables --verbose).", None).unwrap().into(),
                InputOption::new("download-only", None, Some(InputOption::VALUE_NONE), "Download only, do not install packages.", None).unwrap().into(),
                InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "DEPRECATED: Enables installation of require-dev packages (enabled by default, only present for BC).", None).unwrap().into(),
                InputOption::new("no-suggest", None, Some(InputOption::VALUE_NONE), "DEPRECATED: This flag does not exist anymore.", None).unwrap().into(),
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables installation of require-dev packages.", None).unwrap().into(),
                InputOption::new("no-security-blocking", None, Some(InputOption::VALUE_NONE), "Allows installing packages with security advisories or that are abandoned (can also be set via the COMPOSER_NO_SECURITY_BLOCKING=1 env var). Only applies when no lock file is present.", None).unwrap().into(),
                InputOption::new("no-autoloader", None, Some(InputOption::VALUE_NONE), "Skips autoloader generation", None).unwrap().into(),
                InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None).unwrap().into(),
                InputOption::new("no-install", None, Some(InputOption::VALUE_NONE), "Do not use, only defined here to catch misuse of the install command.", None).unwrap().into(),
                InputOption::new("audit", None, Some(InputOption::VALUE_NONE), "Run an audit after installation is complete.", None).unwrap().into(),
                InputOption::new("audit-format", None, Some(InputOption::VALUE_REQUIRED), "Audit output format. Must be \"table\", \"plain\", \"json\", or \"summary\".", Some(PhpMixed::String(Auditor::FORMAT_SUMMARY.to_string()))).unwrap().into(),
                InputOption::new("verbose", Some(PhpMixed::String("v|vv|vvv".to_string())), Some(InputOption::VALUE_NONE), "Shows more details including new commits pulled in when updating packages.", None).unwrap().into(),
                InputOption::new("optimize-autoloader", Some(PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Optimize autoloader during autoloader dump", None).unwrap().into(),
                InputOption::new("classmap-authoritative", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize-autoloader`.", None).unwrap().into(),
                InputOption::new("apcu-autoloader", None, Some(InputOption::VALUE_NONE), "Use APCu to cache found/not-found classes.", None).unwrap().into(),
                InputOption::new("apcu-autoloader-prefix", None, Some(InputOption::VALUE_REQUIRED), "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu-autoloader", None).unwrap().into(),
                InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None).unwrap().into(),
                InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None).unwrap().into(),
                InputArgument::new("packages", Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL), "Should not be provided, use composer require instead to add a given package to composer.json.", None).unwrap().into(),
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

    pub fn execute(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Result<i64> {
        let io = self.get_io().clone();

        if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
            io.write_error("<warning>You are using the deprecated option \"--dev\". It has no effect and will break in Composer 3.</warning>");
        }
        if input
            .borrow()
            .get_option("no-suggest")?
            .as_bool()
            .unwrap_or(false)
        {
            io.write_error("<warning>You are using the deprecated option \"--no-suggest\". It has no effect and will break in Composer 3.</warning>");
        }

        let args = input.borrow().get_argument("packages")?;
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

        if input
            .borrow()
            .get_option("no-install")?
            .as_bool()
            .unwrap_or(false)
        {
            io.write_error("<error>Invalid option \"--no-install\". Use \"composer update --no-install\" instead if you are trying to update the composer.lock file.</error>");
            return Ok(1);
        }

        let composer_handle = self.require_composer(None, None)?;
        let mut composer = crate::command::composer_full_mut(&composer_handle);

        if !composer.get_locker().borrow_mut().is_locked() && !HttpDownloader::is_curl_enabled() {
            io.write_error("<warning>Composer is operating significantly slower than normal because you do not have the PHP curl extension enabled.</warning>");
        }

        // TODO(plugin): dispatch CommandEvent
        let command_event =
            CommandEvent::new(PluginEvents::COMMAND, "install", input.clone(), output);
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None);

        let mut install = Installer::create(io.clone(), &composer_handle);

        let config = composer.get_config();
        let (prefer_source, prefer_dist) =
            self.get_preferred_install_options(&*config.borrow(), input.clone(), false)?;

        let optimize = input
            .borrow()
            .get_option("optimize-autoloader")?
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("optimize-autoloader")
                .as_bool()
                .unwrap_or(false);
        let authoritative = input
            .borrow()
            .get_option("classmap-authoritative")?
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("classmap-authoritative")
                .as_bool()
                .unwrap_or(false);
        let apcu_prefix = input
            .borrow()
            .get_option("apcu-autoloader-prefix")?
            .as_string()
            .map(|s| s.to_string());
        let apcu = apcu_prefix.is_some()
            || input
                .borrow()
                .get_option("apcu-autoloader")?
                .as_bool()
                .unwrap_or(false)
            || config
                .borrow_mut()
                .get("apcu-autoloader")
                .as_bool()
                .unwrap_or(false);

        composer
            .get_installation_manager()
            .borrow_mut()
            .set_output_progress(
                !input
                    .borrow()
                    .get_option("no-progress")?
                    .as_bool()
                    .unwrap_or(false),
            );

        install
            .set_dry_run(
                input
                    .borrow()
                    .get_option("dry-run")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_download_only(
                input
                    .borrow()
                    .get_option("download-only")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_verbose(
                input
                    .borrow()
                    .get_option("verbose")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_prefer_source(prefer_source)
            .set_prefer_dist(prefer_dist)
            .set_dev_mode(
                !input
                    .borrow()
                    .get_option("no-dev")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_dump_autoloader(
                !input
                    .borrow()
                    .get_option("no-autoloader")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_optimize_autoloader(optimize)
            .set_class_map_authoritative(authoritative)
            .set_apcu_autoloader(apcu, apcu_prefix.clone())
            .set_platform_requirement_filter(self.get_platform_requirement_filter(input.clone())?)
            .set_audit_config(
                self.create_audit_config(&mut *composer.get_config().borrow_mut(), input.clone())?,
            )
            .set_error_on_audit(
                input
                    .borrow()
                    .get_option("audit")?
                    .as_bool()
                    .unwrap_or(false),
            );

        if input
            .borrow()
            .get_option("no-plugins")?
            .as_bool()
            .unwrap_or(false)
        {
            install.disable_plugins();
        }

        install.run()
    }
}

impl HasBaseCommandData for InstallCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
