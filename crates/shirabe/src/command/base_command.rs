//! ref: composer/src/Composer/Command/BaseCommand.php
//! ref: composer/vendor/symfony/console/Command/Command.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::component::console::Terminal;
use shirabe_external_packages::symfony::component::console::helper::Table;
use shirabe_external_packages::symfony::component::console::helper::TableSeparator;
use shirabe_external_packages::symfony::component::console::input::InputDefinition;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, RuntimeException, UnexpectedValueException,
    count, explode, in_array, is_string, max,
};

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::command::SelfUpdateCommand;
use crate::composer::Composer;
use crate::config::Config;
use crate::console::Application;
use crate::console::input::InputArgument;
use crate::console::input::InputDefinitionItem;
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::io::NullIO;
use crate::package::version::VersionParser;
use crate::plugin::PluginEvents;
use crate::plugin::PreCommandRunEvent;
use crate::util::Platform;

pub const SUCCESS: i64 = 0;
pub const FAILURE: i64 = 1;
pub const INVALID: i64 = 2;

/// \Composer\Composer\Command\BaseCommand + \Symfony\Component\Console\Command\Command
pub trait BaseCommand {
    fn new(_name: Option<&str>) -> Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn get_name(&self) -> Option<String> {
        todo!()
    }

    fn set_name(&mut self, _name: &str) -> &mut Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn get_description(&self) -> String {
        todo!()
    }

    fn set_description(&mut self, _description: &str) -> &mut Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn set_help(&mut self, _help: &str) -> &mut Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn set_definition(&mut self, _definition: &[InputDefinitionItem]) -> &mut Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn get_definition(&self) -> &InputDefinition {
        todo!()
    }

    fn add_argument(
        &mut self,
        _name: &str,
        _mode: Option<i64>,
        _description: &str,
        _default: PhpMixed,
    ) -> &mut Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn add_option(
        &mut self,
        _name: &str,
        _shortcut: Option<&str>,
        _mode: Option<i64>,
        _description: &str,
        _default: PhpMixed,
    ) -> &mut Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn set_aliases(&mut self, _aliases: &[String]) -> &mut Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn get_aliases(&self) -> Vec<String> {
        todo!()
    }

    fn set_hidden(&mut self, _hidden: bool) -> &mut Self
    where
        Self: Sized,
    {
        todo!()
    }

    fn is_hidden(&self) -> bool {
        todo!()
    }

    fn run(
        &mut self,
        _input: &mut dyn InputInterface,
        _output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        todo!()
    }

    fn get_helper(&self, _name: &str) -> PhpMixed {
        todo!()
    }

    fn get_helper_set(&self) -> PhpMixed {
        todo!()
    }

    /// Gets the application instance for this command.
    fn get_application(&self) -> Result<Application>;

    /// @deprecated since Composer 2.3.0 use requireComposer or tryComposer depending on whether you have $required set to true or false
    fn get_composer(
        &mut self,
        required: bool,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Result<Option<Composer>>;

    /// Retrieves the default Composer\Composer instance or throws
    fn require_composer(
        &mut self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Result<Composer>;

    /// Retrieves the default Composer\Composer instance or null
    fn try_composer(
        &mut self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Option<Composer>;

    fn set_composer(&mut self, composer: Composer);

    /// Removes the cached composer instance
    fn reset_composer(&mut self) -> Result<()>;

    /// Whether or not this command is meant to call another command.
    fn is_proxy_command(&self) -> bool;

    fn get_io(&mut self) -> &mut dyn IOInterface;

    fn set_io(&mut self, io: Box<dyn IOInterface>);

    // TODO(cli-completion): fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions);

    /// @inheritDoc
    fn initialize(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> Result<()>;

    /// Calls {@see Factory::create()} with the given arguments, taking into account flags and default states for disabling scripts and plugins
    fn create_composer_instance(
        &self,
        input: &dyn InputInterface,
        io: &dyn IOInterface,
        config: Option<IndexMap<String, PhpMixed>>,
        disable_plugins: bool,
        disable_scripts: Option<bool>,
    ) -> Result<Composer>;

    /// Returns preferSource and preferDist values based on the configuration.
    fn get_preferred_install_options(
        &self,
        config: &Config,
        input: &dyn InputInterface,
        keep_vcs_requires_prefer_source: bool,
    ) -> Result<(bool, bool)>;

    fn get_platform_requirement_filter(
        &self,
        input: &dyn InputInterface,
    ) -> Result<Box<dyn PlatformRequirementFilterInterface>>;

    /// @param array<string> $requirements
    ///
    /// @return array<string, string>
    fn format_requirements(&self, requirements: Vec<String>) -> Result<IndexMap<String, String>>;

    /// @param array<string> $requirements
    ///
    /// @return list<array{name: string, version?: string}>
    fn normalize_requirements(
        &self,
        requirements: Vec<String>,
    ) -> Result<Vec<IndexMap<String, String>>>;

    /// @param array<TableSeparator|mixed[]> $table
    fn render_table(&self, table: Vec<PhpMixed>, output: &dyn OutputInterface);

    fn get_terminal_width(&self) -> i64;

    /// @internal
    /// @param 'format'|'audit-format' $optName
    /// @return Auditor::FORMAT_*
    fn get_audit_format(&self, input: &dyn InputInterface, opt_name: &str) -> Result<String>;

    /// Creates an AuditConfig from the Config object, optionally overriding security blocking based on input options
    fn create_audit_config(
        &self,
        config: &mut Config,
        input: &dyn InputInterface,
    ) -> Result<AuditConfig>;
}

#[derive(Debug)]
pub struct BaseCommandData {
    pub(crate) composer: Option<Composer>,
    pub(crate) io: Option<Box<dyn IOInterface>>,
}

pub trait HasBaseCommandData {
    fn base_command_data(&self) -> &BaseCommandData;
    fn base_command_data_mut(&mut self) -> &mut BaseCommandData;

    fn composer(&self) -> Option<&Composer> {
        self.base_command_data().composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.base_command_data_mut().composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.base_command_data().io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.base_command_data_mut().io
    }
}

impl<C: HasBaseCommandData> BaseCommand for C {
    fn get_application(&self) -> Result<Application> {
        // TODO(phase-b): requires inner Symfony Command access
        todo!()
    }

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

    fn require_composer(
        &mut self,
        _disable_plugins: Option<bool>,
        _disable_scripts: Option<bool>,
    ) -> Result<Composer> {
        // TODO(phase-b): Composer is a PHP class (shared by reference). Returning owned
        // `Composer` and the Application::get_composer -> &Composer mismatch require a
        // shared-ownership wrapper (Rc<RefCell<Composer>>) before this can be wired up.
        let _ = RuntimeException {
            message: String::new(),
            code: 0,
        };
        todo!("require_composer pending Composer shared-ownership refactor")
    }

    fn try_composer(
        &mut self,
        _disable_plugins: Option<bool>,
        _disable_scripts: Option<bool>,
    ) -> Option<Composer> {
        // TODO(phase-b): same shared-ownership refactor as require_composer.
        todo!("try_composer pending Composer shared-ownership refactor")
    }

    fn set_composer(&mut self, composer: Composer) {
        *self.composer_mut() = Some(composer);
    }

    fn reset_composer(&mut self) -> Result<()> {
        *self.composer_mut() = None;
        self.get_application()?.reset_composer();
        Ok(())
    }

    fn is_proxy_command(&self) -> bool {
        false
    }

    fn get_io(&mut self) -> &mut dyn IOInterface {
        if self.io().is_none() {
            // TODO(phase-b): requires inner Symfony Application access
            *self.io_mut() = Some(Box::new(NullIO::new()));
        }

        &mut **self.io_mut().as_mut().unwrap()
    }

    fn set_io(&mut self, io: Box<dyn IOInterface>) {
        *self.io_mut() = Some(io);
    }

    // TODO(cli-completion): fn complete(&self, input: &CompletionInput, suggestions: &mut CompletionSuggestions)

    fn initialize(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> Result<()> {
        // initialize a plugin-enabled Composer instance, either local or global
        let mut disable_plugins = input.has_parameter_option(&["--no-plugins"], false);
        let mut disable_scripts = input.has_parameter_option(&["--no-scripts"], false);

        // TODO(phase-b): requires inner Symfony Application access for disable_plugins_by_default / disable_scripts_by_default
        // TODO(phase-b): `$this instanceof SelfUpdateCommand` not representable

        let composer = self.try_composer(Some(disable_plugins), Some(disable_scripts));
        // TODO(phase-b): re-borrow self for get_io after try_composer move
        let io_ptr: *const dyn IOInterface = self.get_io();
        let io = unsafe { &*io_ptr };

        let disable_plugins_kind = if disable_plugins {
            crate::factory::DisablePlugins::All
        } else {
            crate::factory::DisablePlugins::None
        };
        let composer = if composer.is_none() {
            Factory::create_global(io, disable_plugins_kind, disable_scripts)
        } else {
            composer
        };
        if let Some(composer) = composer.as_ref() {
            // TODO(phase-b): requires inner Symfony Command get_name access
            let command_name: String = todo!();
            let pre_command_run_event = PreCommandRunEvent::new(
                PluginEvents::PRE_COMMAND_RUN.to_string(),
                input,
                command_name,
            );
            // TODO(phase-b): event_dispatcher.dispatch expects Option<Event>; need wrapper from
            // PreCommandRunEvent.
            let _ = composer.get_event_dispatcher();
            let _ = pre_command_run_event.get_name();
        }

        if input.has_parameter_option(&["--no-ansi"], false) && input.has_option("no-progress") {
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
                        && Platform::get_env(env_name).map_or(false, |s| !s.is_empty() && s != "0")
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
                    .map_or(false, |s| !s.is_empty() && s != "0")
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
            let ignore_str = ignore_platform_req_env.clone().unwrap_or_default();
            if 0 == count(&input.get_option("ignore-platform-req"))
                && ignore_platform_req_env.is_some()
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

        // TODO(phase-b): requires inner Symfony Command initialize
        Ok(())
    }

    fn create_composer_instance(
        &self,
        input: &dyn InputInterface,
        io: &dyn IOInterface,
        config: Option<IndexMap<String, PhpMixed>>,
        disable_plugins: bool,
        disable_scripts: Option<bool>,
    ) -> Result<Composer> {
        let disable_plugins =
            disable_plugins || input.has_parameter_option(&["--no-plugins"], false);
        let disable_scripts = disable_scripts.unwrap_or(false)
            || input.has_parameter_option(&["--no-scripts"], false);

        // TODO(phase-b): requires inner Symfony Application access for disable_plugins_by_default / disable_scripts_by_default
        let disable_plugins_kind = if disable_plugins {
            crate::factory::DisablePlugins::All
        } else {
            crate::factory::DisablePlugins::None
        };
        // TODO(phase-b): Option<IndexMap<String, PhpMixed>> -> Option<LocalConfigInput> conversion
        let _ = config;
        Factory::create(io, None, disable_plugins_kind, disable_scripts)
    }

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
            return Ok(PlatformRequirementFilterFactory::from_bool_or_list(
                ignores,
            )?);
        }

        Ok(PlatformRequirementFilterFactory::ignore_nothing())
    }

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

    fn normalize_requirements(
        &self,
        requirements: Vec<String>,
    ) -> Result<Vec<IndexMap<String, String>>> {
        // TODO(phase-b): VersionParser has no public `new` yet
        let parser: VersionParser = todo!("VersionParser::new()");

        parser.parse_name_version_pairs(requirements)
    }

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

    fn create_audit_config(
        &self,
        config: &mut Config,
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
            .map_or(false, |s| !s.is_empty() && s != "0")
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

// TODO(phase-b): bridge BaseCommand to Symfony Command for trait-object container usage.
// Cannot blanket-impl a foreign trait for a local generic (orphan rule); each concrete
// command must impl symfony Command itself, or a wrapper type must be introduced.
