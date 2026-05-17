//! ref: composer/src/Composer/Command/BaseCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::completion::completion_input::CompletionInput;
use shirabe_external_packages::symfony::component::console::completion::completion_suggestions::CompletionSuggestions;
use shirabe_external_packages::symfony::component::console::helper::table::Table;
use shirabe_external_packages::symfony::component::console::helper::table_separator::TableSeparator;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_external_packages::symfony::component::console::terminal::Terminal;
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, RuntimeException, UnexpectedValueException,
    count, explode, in_array, is_string, max,
};

use crate::advisory::audit_config::AuditConfig;
use crate::advisory::auditor::Auditor;
use crate::command::self_update_command::SelfUpdateCommand;
use crate::composer::Composer;
use crate::config::Config;
use crate::console::application::Application;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::factory::Factory;
use crate::filter::platform_requirement_filter::platform_requirement_filter_factory::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::platform_requirement_filter_interface::PlatformRequirementFilterInterface;
use crate::io::io_interface::IOInterface;
use crate::io::null_io::NullIO;
use crate::package::version::version_parser::VersionParser;
use crate::plugin::plugin_events::PluginEvents;
use crate::plugin::pre_command_run_event::PreCommandRunEvent;
use crate::util::platform::Platform;

/// Base class for Composer commands
pub trait BaseCommand {
    fn inner(&self) -> &Command;
    fn inner_mut(&mut self) -> &mut Command;
    fn composer(&self) -> Option<&Composer>;
    fn composer_mut(&mut self) -> &mut Option<Composer>;
    fn io(&self) -> Option<&dyn IOInterface>;
    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>>;

    /// Gets the application instance for this command.
    fn get_application(&self) -> Result<Application> {
        let application = self.inner().get_application();
        // TODO(phase-b): `$application instanceof Application` downcast from generic Symfony Application
        let application_as_composer: Option<Application> = application;
        if application_as_composer.is_none() {
            return Err(RuntimeException {
                message: format!(
                    "Composer commands can only work with an {} instance set",
                    "Composer\\Console\\Application"
                ),
                code: 0,
            }
            .into());
        }

        Ok(application_as_composer.unwrap())
    }

    /// @deprecated since Composer 2.3.0 use requireComposer or tryComposer depending on whether you have $required set to true or false
    fn get_composer(
        &mut self,
        required: bool,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Result<Option<Composer>> {
        if required {
            return Ok(Some(
                self.require_composer(disable_plugins, disable_scripts)?,
            ));
        }

        Ok(self.try_composer(disable_plugins, disable_scripts))
    }

    /// Retrieves the default Composer\Composer instance or throws
    fn require_composer(
        &mut self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Result<Composer> {
        if self.composer().is_none() {
            let application = self.inner().get_application();
            // TODO(phase-b): `$application instanceof Application` downcast
            let application_as_composer: Option<Application> = application;
            if let Some(app) = application_as_composer {
                *self.composer_mut() =
                    Some(app.get_composer(true, disable_plugins, disable_scripts)?);
                // PHP: assert($this->composer instanceof Composer) — Rust types guarantee this
            } else {
                return Err(RuntimeException {
                    message:
                        "Could not create a Composer\\Composer instance, you must inject one if this command is not used with a Composer\\Console\\Application instance"
                            .to_string(),
                    code: 0,
                }
                .into());
            }
        }

        Ok(self.composer().clone().unwrap())
    }

    /// Retrieves the default Composer\Composer instance or null
    fn try_composer(
        &mut self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Option<Composer> {
        if self.composer().is_none() {
            let application = self.inner().get_application();
            // TODO(phase-b): `$application instanceof Application` downcast
            let application_as_composer: Option<Application> = application;
            if let Some(app) = application_as_composer {
                *self.composer_mut() = app
                    .get_composer(false, disable_plugins, disable_scripts)
                    .ok();
            }
        }

        self.composer().clone()
    }

    fn set_composer(&mut self, composer: Composer) {
        *self.composer_mut() = Some(composer);
    }

    /// Removes the cached composer instance
    fn reset_composer(&mut self) -> Result<()> {
        *self.composer_mut() = None;
        self.get_application()?.reset_composer();
        Ok(())
    }

    /// Whether or not this command is meant to call another command.
    fn is_proxy_command(&self) -> bool {
        false
    }

    fn get_io(&mut self) -> &dyn IOInterface {
        if self.io().is_none() {
            let application = self.inner().get_application();
            // TODO(phase-b): `$application instanceof Application` downcast
            let application_as_composer: Option<Application> = application;
            if let Some(app) = application_as_composer {
                *self.io_mut() = Some(app.get_io());
            } else {
                *self.io_mut() = Some(Box::new(NullIO::new()));
            }
        }

        &**self.io().as_ref().unwrap()
    }

    fn set_io(&mut self, io: Box<dyn IOInterface>) {
        *self.io_mut() = Some(io);
    }

    /// @inheritdoc
    ///
    /// Backport suggested values definition from symfony/console 6.1+
    fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        let definition = self.inner().get_definition();
        let name = input.get_completion_name().to_string();
        if CompletionInput::TYPE_OPTION_VALUE == input.get_completion_type()
            && definition.has_option(&name)
        {
            let option = definition.get_option(&name);
            // TODO(phase-b): `$option instanceof InputOption` (our InputOption, not Symfony's)
            let option_as_input: Option<&InputOption> = None;
            if let Some(input_option) = option_as_input {
                input_option.complete(input, suggestions);
                let _ = option;
                return;
            }
        }
        if CompletionInput::TYPE_ARGUMENT_VALUE == input.get_completion_type()
            && definition.has_argument(&name)
        {
            let argument = definition.get_argument(&name);
            // TODO(phase-b): `$argument instanceof InputArgument` (our InputArgument, not Symfony's)
            let argument_as_input: Option<&InputArgument> = None;
            if let Some(input_argument) = argument_as_input {
                input_argument.complete(input, suggestions);
                let _ = argument;
                return;
            }
        }
        self.inner().complete(input, suggestions);
    }

    /// @inheritDoc
    fn initialize(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> Result<()> {
        // initialize a plugin-enabled Composer instance, either local or global
        let mut disable_plugins =
            input.has_parameter_option(PhpMixed::String("--no-plugins".to_string()));
        let mut disable_scripts =
            input.has_parameter_option(PhpMixed::String("--no-scripts".to_string()));

        let application = self.inner().get_application();
        // TODO(phase-b): `$application instanceof Application` downcast
        let application_as_composer: Option<&Application> = None;
        if let Some(app) = application_as_composer {
            if app.get_disable_plugins_by_default() {
                disable_plugins = true;
            }
            if app.get_disable_scripts_by_default() {
                disable_scripts = true;
            }
        }
        let _ = application;

        // TODO(phase-b): `$this instanceof SelfUpdateCommand` — not representable since
        // BaseCommand is a struct, not a base type
        let self_is_self_update: Option<&SelfUpdateCommand> = None;
        if self_is_self_update.is_some() {
            disable_plugins = true;
            disable_scripts = true;
        }

        let composer = self.try_composer(Some(disable_plugins), Some(disable_scripts));
        // TODO(phase-b): re-borrow self for get_io after try_composer move
        let io_ptr: *const dyn IOInterface = self.get_io();
        let io = unsafe { &*io_ptr };

        let composer = if composer.is_none() {
            Some(Factory::create_global(
                io,
                Some(disable_plugins),
                Some(disable_scripts),
            )?)
        } else {
            composer
        };
        if let Some(composer) = composer.as_ref() {
            let pre_command_run_event = PreCommandRunEvent::new(
                PluginEvents::PRE_COMMAND_RUN.to_string(),
                Box::new(input),
                self.inner().get_name().to_string(),
            );
            composer.get_event_dispatcher().dispatch(
                pre_command_run_event.get_name(),
                Box::new(pre_command_run_event),
            );
        }

        if true
            == input.has_parameter_option(PhpMixed::List(vec![Box::new(PhpMixed::String(
                "--no-ansi".to_string(),
            ))]))
            && input.has_option("no-progress")
        {
            input.set_option("no-progress", PhpMixed::Bool(true));
        }

        let env_options: IndexMap<&str, Vec<&str>> = [
            ("COMPOSER_NO_AUDIT", vec!["no-audit"]),
            ("COMPOSER_NO_DEV", vec!["no-dev", "update-no-dev"]),
            ("COMPOSER_PREFER_STABLE", vec!["prefer-stable"]),
            ("COMPOSER_PREFER_LOWEST", vec!["prefer-lowest"]),
            ("COMPOSER_MINIMAL_CHANGES", vec!["minimal-changes"]),
            ("COMPOSER_WITH_DEPENDENCIES", vec!["with-dependencies"]),
            (
                "COMPOSER_WITH_ALL_DEPENDENCIES",
                vec!["with-all-dependencies"],
            ),
            (
                "COMPOSER_NO_SECURITY_BLOCKING",
                vec!["no-security-blocking"],
            ),
        ]
        .into_iter()
        .collect();
        for (env_name, option_names) in &env_options {
            for option_name in option_names {
                if true == input.has_option(option_name) {
                    if false == input.get_option(option_name).as_bool().unwrap_or(false)
                        && Platform::get_env(env_name).as_bool().unwrap_or(false)
                    {
                        input.set_option(option_name, PhpMixed::Bool(true));
                    }
                }
            }
        }

        if true == input.has_option("ignore-platform-reqs") {
            if !input
                .get_option("ignore-platform-reqs")
                .as_bool()
                .unwrap_or(false)
                && Platform::get_env("COMPOSER_IGNORE_PLATFORM_REQS")
                    .as_bool()
                    .unwrap_or(false)
            {
                input.set_option("ignore-platform-reqs", PhpMixed::Bool(true));

                io.write_error("<warning>COMPOSER_IGNORE_PLATFORM_REQS is set. You may experience unexpected errors.</warning>");
            }
        }

        if true == input.has_option("ignore-platform-req")
            && (!input.has_option("ignore-platform-reqs")
                || !input
                    .get_option("ignore-platform-reqs")
                    .as_bool()
                    .unwrap_or(false))
        {
            let ignore_platform_req_env = Platform::get_env("COMPOSER_IGNORE_PLATFORM_REQ");
            let ignore_str = ignore_platform_req_env
                .as_string()
                .unwrap_or("")
                .to_string();
            if 0 == count(&input.get_option("ignore-platform-req"))
                && is_string(&ignore_platform_req_env)
                && "" != ignore_str
            {
                input.set_option(
                    "ignore-platform-req",
                    PhpMixed::List(
                        explode(",", &ignore_str)
                            .into_iter()
                            .map(|s| Box::new(PhpMixed::String(s)))
                            .collect(),
                    ),
                );

                io.write_error(&format!(
                    "<warning>COMPOSER_IGNORE_PLATFORM_REQ is set to ignore {}. You may experience unexpected errors.</warning>",
                    ignore_str
                ));
            }
        }

        self.inner().initialize(input, output)
    }

    /// Calls {@see Factory::create()} with the given arguments, taking into account flags and default states for disabling scripts and plugins
    fn create_composer_instance(
        &self,
        input: &dyn InputInterface,
        io: &dyn IOInterface,
        config: Option<IndexMap<String, PhpMixed>>,
        disable_plugins: bool,
        disable_scripts: Option<bool>,
    ) -> Result<Composer> {
        let mut disable_plugins = disable_plugins == true
            || input.has_parameter_option(PhpMixed::String("--no-plugins".to_string()));
        let mut disable_scripts = disable_scripts == Some(true)
            || input.has_parameter_option(PhpMixed::String("--no-scripts".to_string()));

        let application = self.inner().get_application();
        // TODO(phase-b): `$application instanceof Application` downcast
        let application_as_composer: Option<&Application> = None;
        if let Some(app) = application_as_composer {
            if app.get_disable_plugins_by_default() {
                disable_plugins = true;
            }
            if app.get_disable_scripts_by_default() {
                disable_scripts = true;
            }
        }
        let _ = application;

        Factory::create(io, config, disable_plugins, disable_scripts)
    }

    /// Returns preferSource and preferDist values based on the configuration.
    fn get_preferred_install_options(
        &self,
        config: &Config,
        input: &dyn InputInterface,
        keep_vcs_requires_prefer_source: bool,
    ) -> Result<(bool, bool)> {
        let mut prefer_source = false;
        let mut prefer_dist = false;

        match config.get("preferred-install").as_string().unwrap_or("") {
            "source" => {
                prefer_source = true;
            }
            "dist" => {
                prefer_dist = true;
            }
            "auto" | _ => {
                // noop
            }
        }

        if !input.has_option("prefer-dist") || !input.has_option("prefer-source") {
            return Ok((prefer_source, prefer_dist));
        }

        if input.has_option("prefer-install") && is_string(&input.get_option("prefer-install")) {
            if input.get_option("prefer-source").as_bool().unwrap_or(false) {
                return Err(InvalidArgumentException {
                    message: "--prefer-source can not be used together with --prefer-install"
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            if input.get_option("prefer-dist").as_bool().unwrap_or(false) {
                return Err(InvalidArgumentException {
                    message: "--prefer-dist can not be used together with --prefer-install"
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            let prefer_install = input.get_option("prefer-install");
            match prefer_install.as_string().unwrap_or("") {
                "dist" => {
                    // TODO(phase-b): InputInterface set_option needs &mut self
                    let _ = "input.set_option('prefer-dist', true)";
                }
                "source" => {
                    let _ = "input.set_option('prefer-source', true)";
                }
                "auto" => {
                    prefer_dist = false;
                    prefer_source = false;
                }
                other => {
                    return Err(UnexpectedValueException {
                        message: format!(
                            "--prefer-install accepts one of \"dist\", \"source\" or \"auto\", got {}",
                            other
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
        }

        if input.get_option("prefer-source").as_bool().unwrap_or(false)
            || input.get_option("prefer-dist").as_bool().unwrap_or(false)
            || (keep_vcs_requires_prefer_source
                && input.has_option("keep-vcs")
                && input.get_option("keep-vcs").as_bool().unwrap_or(false))
        {
            prefer_source = input.get_option("prefer-source").as_bool().unwrap_or(false)
                || (keep_vcs_requires_prefer_source
                    && input.has_option("keep-vcs")
                    && input.get_option("keep-vcs").as_bool().unwrap_or(false));
            prefer_dist = input.get_option("prefer-dist").as_bool().unwrap_or(false);
        }

        Ok((prefer_source, prefer_dist))
    }

    fn get_platform_requirement_filter(
        &self,
        input: &dyn InputInterface,
    ) -> Result<Box<dyn PlatformRequirementFilterInterface>> {
        if !input.has_option("ignore-platform-reqs") || !input.has_option("ignore-platform-req") {
            return Err(LogicException {
                message:
                    "Calling getPlatformRequirementFilter from a command which does not define the --ignore-platform-req[s] flags is not permitted."
                        .to_string(),
                code: 0,
            }
            .into());
        }

        if true
            == input
                .get_option("ignore-platform-reqs")
                .as_bool()
                .unwrap_or(false)
        {
            return Ok(PlatformRequirementFilterFactory::ignore_all());
        }

        let ignores = input.get_option("ignore-platform-req");
        if count(&ignores) > 0 {
            return Ok(PlatformRequirementFilterFactory::from_bool_or_list(ignores));
        }

        Ok(PlatformRequirementFilterFactory::ignore_nothing())
    }

    /// @param array<string> $requirements
    ///
    /// @return array<string, string>
    fn format_requirements(&self, requirements: Vec<String>) -> Result<IndexMap<String, String>> {
        let mut requires: IndexMap<String, String> = IndexMap::new();
        let requirements = self.normalize_requirements(requirements)?;
        for requirement in requirements {
            if !requirement.contains_key("version") {
                return Err(UnexpectedValueException {
                    message: format!(
                        "Option {} is missing a version constraint, use e.g. {}:^1.0",
                        requirement.get("name").map(|s| s.as_str()).unwrap_or(""),
                        requirement.get("name").map(|s| s.as_str()).unwrap_or(""),
                    ),
                    code: 0,
                }
                .into());
            }
            requires.insert(
                requirement.get("name").cloned().unwrap_or_default(),
                requirement.get("version").cloned().unwrap_or_default(),
            );
        }

        Ok(requires)
    }

    /// @param array<string> $requirements
    ///
    /// @return list<array{name: string, version?: string}>
    fn normalize_requirements(
        &self,
        requirements: Vec<String>,
    ) -> Result<Vec<IndexMap<String, String>>> {
        // TODO(phase-b): VersionParser has no public `new` yet
        let parser: VersionParser = todo!("VersionParser::new()");

        parser.parse_name_version_pairs(requirements)
    }

    /// @param array<TableSeparator|mixed[]> $table
    fn render_table(&self, table: Vec<PhpMixed>, output: &dyn OutputInterface) {
        let mut renderer = Table::new(output);
        renderer.set_style("compact");
        renderer.set_rows(table).render();
        let _ = TableSeparator::new();
    }

    fn get_terminal_width(&self) -> i64 {
        let terminal = Terminal::new();
        let mut width = terminal.get_width();

        if Platform::is_windows() {
            width -= 1;
        } else {
            width = max(80, width);
        }

        width
    }

    /// @internal
    /// @param 'format'|'audit-format' $optName
    /// @return Auditor::FORMAT_*
    fn get_audit_format(&self, input: &dyn InputInterface, opt_name: &str) -> Result<String> {
        if !input.has_option(opt_name) {
            return Err(LogicException {
                message: format!(
                    "This should not be called on a Command which has no {} option defined.",
                    opt_name
                ),
                code: 0,
            }
            .into());
        }

        let val = input.get_option(opt_name);
        let formats: Vec<Box<PhpMixed>> = Auditor::FORMATS
            .iter()
            .map(|s| Box::new(PhpMixed::String(s.to_string())))
            .collect();
        if !in_array(val.clone(), &PhpMixed::List(formats), true) {
            return Err(InvalidArgumentException {
                message: format!(
                    "--{} must be one of {}.",
                    opt_name,
                    Auditor::FORMATS.join(", ")
                ),
                code: 0,
            }
            .into());
        }

        Ok(val.as_string().unwrap_or("").to_string())
    }

    /// Creates an AuditConfig from the Config object, optionally overriding security blocking based on input options
    fn create_audit_config(
        &self,
        config: &Config,
        input: &dyn InputInterface,
    ) -> Result<AuditConfig> {
        // Handle both --audit and --no-audit flags
        let audit = if input.has_option("audit") {
            input.get_option("audit").as_bool().unwrap_or(false)
        } else {
            !(input.has_option("no-audit")
                && input.get_option("no-audit").as_bool().unwrap_or(false))
        };
        let audit_format = if input.has_option("audit-format") {
            self.get_audit_format(input, "audit-format")?
        } else {
            Auditor::FORMAT_SUMMARY.to_string()
        };

        let audit_config = AuditConfig::from_config(config, audit, &audit_format)?;

        if Platform::get_env("COMPOSER_NO_SECURITY_BLOCKING")
            .as_bool()
            .unwrap_or(false)
            || (input.has_option("no-security-blocking")
                && input
                    .get_option("no-security-blocking")
                    .as_bool()
                    .unwrap_or(false))
        {
            let audit_config = AuditConfig::new(
                audit_config.audit,
                audit_config.audit_format,
                audit_config.audit_abandoned,
                false, // blockInsecure
                audit_config.block_abandoned,
                audit_config.ignore_unreachable,
                audit_config.ignore_list_for_audit,
                audit_config.ignore_list_for_blocking,
                audit_config.ignore_severity_for_audit,
                audit_config.ignore_severity_for_blocking,
                audit_config.ignore_abandoned_for_audit,
                audit_config.ignore_abandoned_for_blocking,
            );
            return Ok(audit_config);
        }

        Ok(audit_config)
    }
}
