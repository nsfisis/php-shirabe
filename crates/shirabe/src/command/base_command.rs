//! ref: composer/src/Composer/Command/BaseCommand.php
//! ref: composer/vendor/symfony/console/Command/Command.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::console::Terminal;
use shirabe_external_packages::symfony::console::command::command::{
    Command, CommandData, SetDefinitionArg,
};
use shirabe_external_packages::symfony::console::helper::Table;
use shirabe_external_packages::symfony::console::helper::TableSeparator;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{
    AsAny, InvalidArgumentException, LogicException, PhpMixed, RuntimeException,
    UnexpectedValueException, count, explode, in_array, is_string,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::Application;
use crate::console::input::InputArgument;
use crate::console::input::InputDefinitionItem;
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::NullIO;
use crate::package::version::VersionParser;
use crate::plugin::PluginEvents;
use crate::plugin::PreCommandRunEvent;
use crate::util::Platform;

pub const SUCCESS: i64 = 0;
pub const FAILURE: i64 = 1;
pub const INVALID: i64 = 2;

/// The base-class state shared by all Composer commands: the embedded Symfony command
/// state plus the lazily-resolved Composer instance and IO.
#[derive(Debug)]
pub struct BaseCommandData {
    inner: CommandData,
    pub(crate) composer: RefCell<Option<PartialComposerHandle>>,
    pub(crate) io: RefCell<Option<Rc<RefCell<dyn IOInterface>>>>,
}

impl BaseCommandData {
    pub fn new(name: Option<String>) -> Self {
        BaseCommandData {
            inner: CommandData::new(name),
            composer: RefCell::new(None),
            io: RefCell::new(None),
        }
    }

    /// Access to the embedded Symfony command state, used by the Composer-typed definition
    /// builders to forward to `CommandData`'s Symfony-typed entry points. `CommandData` is
    /// interior-mutable, so a shared reference is enough.
    pub(crate) fn command_data(&self) -> &CommandData {
        &self.inner
    }
}

/// \Composer\Composer\Command\BaseCommand — the Composer additions on top of the Symfony
/// `Command` trait. The Symfony state methods are inherited from the [`Command`] supertrait;
/// only the Composer-specific behavior and the Composer-typed definition builders live here.
pub trait BaseCommand: Command {
    /// Access to the embedded Symfony command state. Each command returns
    /// `self.base_command_data.command_data()`; this lets the Composer-typed definition
    /// builders below forward to `CommandData`'s Symfony-typed entry points. `CommandData` is
    /// interior-mutable, so a shared reference is enough.
    fn command_data(&self) -> &CommandData;

    /// Sets the definition from Composer-typed argument/option instances.
    fn set_definition(&self, definition: &[InputDefinitionItem]) -> &Self
    where
        Self: Sized,
    {
        let items = definition
            .iter()
            .map(|item| item.to_definition_item())
            .collect();
        self.command_data()
            .set_definition(SetDefinitionArg::Array(items));
        self
    }

    fn add_argument(
        &self,
        name: &str,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> &Self
    where
        Self: Sized,
    {
        self.command_data()
            .add_argument(name, mode, description, default)
            .expect("command argument definitions in configure() are statically valid");
        self
    }

    fn add_option(
        &self,
        name: &str,
        shortcut: Option<&str>,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> &Self
    where
        Self: Sized,
    {
        let shortcut = shortcut
            .map(|s| PhpMixed::from(s.to_string()))
            .unwrap_or(PhpMixed::Null);
        self.command_data()
            .add_option(name, shortcut, mode, description, default)
            .expect("command option definitions in configure() are statically valid");
        self
    }

    /// Whether this command is the self-update command (disables plugins/scripts).
    fn is_self_update_command(&self) -> bool {
        false
    }

    /// Whether or not this command is meant to call another command.
    fn is_proxy_command(&self) -> bool {
        false
    }

    /// Retrieves the default Composer\Composer instance or throws
    fn require_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Result<PartialComposerHandle>;

    /// Retrieves the default Composer\Composer instance or null
    fn try_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Option<PartialComposerHandle>;

    fn set_composer(&self, composer: PartialComposerHandle);

    /// Removes the cached composer instance
    fn reset_composer(&self) -> Result<()>;

    fn get_io(&self) -> Rc<RefCell<dyn IOInterface>>;

    fn set_io(&self, io: Rc<RefCell<dyn IOInterface>>);

    /// Calls {@see Factory::create()} with the given arguments, taking into account flags and default states for disabling scripts and plugins
    fn create_composer_instance(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        io: Rc<RefCell<dyn IOInterface>>,
        config: Option<IndexMap<String, PhpMixed>>,
        disable_plugins: bool,
        disable_scripts: Option<bool>,
    ) -> Result<PartialComposerHandle>;

    /// Returns preferSource and preferDist values based on the configuration.
    fn get_preferred_install_options(
        &self,
        config: &Config,
        input: Rc<RefCell<dyn InputInterface>>,
        keep_vcs_requires_prefer_source: bool,
    ) -> Result<(bool, bool)>;

    fn get_platform_requirement_filter(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
    ) -> Result<Rc<dyn PlatformRequirementFilterInterface>>;

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
    fn render_table(&self, table: Vec<PhpMixed>, output: Rc<RefCell<dyn OutputInterface>>);

    fn get_terminal_width(&self) -> i64;

    /// @internal
    /// @param 'format'|'audit-format' $optName
    /// @return Auditor::FORMAT_*
    fn get_audit_format(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        opt_name: &str,
    ) -> Result<String>;

    /// Creates an AuditConfig from the Config object, optionally overriding security blocking based on input options
    fn create_audit_config(
        &self,
        config: &mut Config,
        input: Rc<RefCell<dyn InputInterface>>,
    ) -> Result<AuditConfig>;
}

/// Forwards every Composer-specific `BaseCommand` method that no command overrides to an
/// embedded field. Each command invokes this once inside its `impl BaseCommand` block,
/// alongside its hand-written `command_data_mut` (and any overridden behavior hooks). The
/// single argument names the field to forward to (always `base_command_data`).
#[macro_export]
macro_rules! delegate_base_command_trait_impls_to_inner {
    ($field:ident) => {
        shirabe_external_packages::delegate_to_inner!($field, fn require_composer(&self, disable_plugins: Option<bool>, disable_scripts: Option<bool>) -> anyhow::Result<$crate::composer::PartialComposerHandle>);
        shirabe_external_packages::delegate_to_inner!($field, fn try_composer(&self, disable_plugins: Option<bool>, disable_scripts: Option<bool>) -> Option<$crate::composer::PartialComposerHandle>);
        shirabe_external_packages::delegate_to_inner!($field, fn set_composer(&self, composer: $crate::composer::PartialComposerHandle));
        shirabe_external_packages::delegate_to_inner!($field, fn reset_composer(&self) -> anyhow::Result<()>);
        shirabe_external_packages::delegate_to_inner!($field, fn get_io(&self) -> std::rc::Rc<std::cell::RefCell<dyn $crate::io::IOInterface>>);
        shirabe_external_packages::delegate_to_inner!($field, fn set_io(&self, io: std::rc::Rc<std::cell::RefCell<dyn $crate::io::IOInterface>>));
        shirabe_external_packages::delegate_to_inner!($field, fn create_composer_instance(&self, input: std::rc::Rc<std::cell::RefCell<dyn shirabe_external_packages::symfony::console::input::InputInterface>>, io: std::rc::Rc<std::cell::RefCell<dyn $crate::io::IOInterface>>, config: Option<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>>, disable_plugins: bool, disable_scripts: Option<bool>) -> anyhow::Result<$crate::composer::PartialComposerHandle>);
        shirabe_external_packages::delegate_to_inner!($field, fn get_preferred_install_options(&self, config: &$crate::config::Config, input: std::rc::Rc<std::cell::RefCell<dyn shirabe_external_packages::symfony::console::input::InputInterface>>, keep_vcs_requires_prefer_source: bool) -> anyhow::Result<(bool, bool)>);
        shirabe_external_packages::delegate_to_inner!($field, fn get_platform_requirement_filter(&self, input: std::rc::Rc<std::cell::RefCell<dyn shirabe_external_packages::symfony::console::input::InputInterface>>) -> anyhow::Result<std::rc::Rc<dyn $crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface>>);
        shirabe_external_packages::delegate_to_inner!($field, fn format_requirements(&self, requirements: Vec<String>) -> anyhow::Result<indexmap::IndexMap<String, String>>);
        shirabe_external_packages::delegate_to_inner!($field, fn normalize_requirements(&self, requirements: Vec<String>) -> anyhow::Result<Vec<indexmap::IndexMap<String, String>>>);
        shirabe_external_packages::delegate_to_inner!($field, fn render_table(&self, table: Vec<shirabe_php_shim::PhpMixed>, output: std::rc::Rc<std::cell::RefCell<dyn shirabe_external_packages::symfony::console::output::OutputInterface>>));
        shirabe_external_packages::delegate_to_inner!($field, fn get_terminal_width(&self) -> i64);
        shirabe_external_packages::delegate_to_inner!($field, fn get_audit_format(&self, input: std::rc::Rc<std::cell::RefCell<dyn shirabe_external_packages::symfony::console::input::InputInterface>>, opt_name: &str) -> anyhow::Result<String>);
        shirabe_external_packages::delegate_to_inner!($field, fn create_audit_config(&self, config: &mut $crate::config::Config, input: std::rc::Rc<std::cell::RefCell<dyn shirabe_external_packages::symfony::console::input::InputInterface>>) -> anyhow::Result<$crate::advisory::AuditConfig>);
    };
}

impl Command for BaseCommandData {
    shirabe_external_packages::delegate_command_trait_impls_to_inner!(inner);
}

impl BaseCommand for BaseCommandData {
    fn command_data(&self) -> &CommandData {
        &self.inner
    }

    fn require_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Result<PartialComposerHandle> {
        if self.composer.borrow().is_none() {
            let application = self.get_application();
            let Some(application) = application else {
                return Err(RuntimeException {
                    message: "Could not create a Composer\\Composer instance, you must inject one if this command is not used with a Composer\\Console\\Application instance".to_string(),
                    code: 0,
                }
                .into());
            };
            let composer = {
                let mut app_ref = application.borrow_mut();
                let app_dyn: &mut dyn shirabe_external_packages::symfony::console::application::Application = &mut *app_ref;
                let app = app_dyn
                    .as_any_mut()
                    .downcast_mut::<Application>()
                    .expect("a Composer command's application is a shirabe Application");
                app.get_composer(true, disable_plugins, disable_scripts)?
            };
            *self.composer.borrow_mut() = composer;
        }

        Ok(self
            .composer
            .borrow()
            .clone()
            .expect("requireComposer always yields a Composer or errors"))
    }

    fn try_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Option<PartialComposerHandle> {
        if self.composer.borrow().is_none()
            && let Some(application) = self.get_application()
        {
            let result = {
                let mut app_ref = application.borrow_mut();
                let app_dyn: &mut dyn shirabe_external_packages::symfony::console::application::Application = &mut *app_ref;
                let app = app_dyn
                    .as_any_mut()
                    .downcast_mut::<Application>()
                    .expect("a Composer command's application is a shirabe Application");
                app.get_composer(false, disable_plugins, disable_scripts)
            };
            if let Ok(composer) = result {
                *self.composer.borrow_mut() = composer;
            }
        }

        self.composer.borrow().clone()
    }

    fn set_composer(&self, composer: PartialComposerHandle) {
        *self.composer.borrow_mut() = Some(composer);
    }

    fn reset_composer(&self) -> Result<()> {
        *self.composer.borrow_mut() = None;
        if let Some(application) = self.get_application() {
            let mut app_ref = application.borrow_mut();
            let app_dyn: &mut dyn shirabe_external_packages::symfony::console::application::Application = &mut *app_ref;
            let app = app_dyn
                .as_any_mut()
                .downcast_mut::<Application>()
                .expect("a Composer command's application is a shirabe Application");
            app.reset_composer();
        }
        Ok(())
    }

    fn get_io(&self) -> Rc<RefCell<dyn IOInterface>> {
        if self.io.borrow().is_none() {
            match self.get_application() {
                Some(application) => {
                    let io = {
                        let app_ref = application.borrow();
                        let app_dyn: &dyn shirabe_external_packages::symfony::console::application::Application = &*app_ref;
                        let app = app_dyn
                            .as_any()
                            .downcast_ref::<Application>()
                            .expect("a Composer command's application is a shirabe Application");
                        app.get_io()
                    };
                    *self.io.borrow_mut() = Some(io);
                }
                None => {
                    *self.io.borrow_mut() = Some(Rc::new(RefCell::new(NullIO::new())));
                }
            }
        }

        self.io.borrow().clone().unwrap()
    }

    fn set_io(&self, io: Rc<RefCell<dyn IOInterface>>) {
        *self.io.borrow_mut() = Some(io);
    }

    fn create_composer_instance(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        io: Rc<RefCell<dyn IOInterface>>,
        config: Option<IndexMap<String, PhpMixed>>,
        disable_plugins: bool,
        disable_scripts: Option<bool>,
    ) -> Result<PartialComposerHandle> {
        let disable_plugins = disable_plugins
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from(vec!["--no-plugins"]), false);
        let disable_scripts = disable_scripts.unwrap_or(false)
            || input
                .borrow()
                .has_parameter_option(PhpMixed::from(vec!["--no-scripts"]), false);

        // PHP: if ($app instanceof Application && $app->getDisablePluginsByDefault()) $disablePlugins = true;
        //      (same for getDisableScriptsByDefault()).
        // TODO(phase-c): these application-default overrides need a shared Application handle
        // (deferred), so only the passed/flag values apply.
        let disable_plugins_kind = if disable_plugins {
            crate::factory::DisablePlugins::All
        } else {
            crate::factory::DisablePlugins::None
        };
        let config = config.map(crate::factory::LocalConfigInput::Data);
        Factory::create(io, config, disable_plugins_kind, disable_scripts).map(|c| c.upcast())
    }

    fn get_preferred_install_options(
        &self,
        config: &Config,
        input: Rc<RefCell<dyn InputInterface>>,
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
            _ => {
                // noop
            }
        }

        if !input.borrow().has_option("prefer-dist") || !input.borrow().has_option("prefer-source")
        {
            return Ok((prefer_source, prefer_dist));
        }

        if input.borrow().has_option("prefer-install")
            && is_string(&input.borrow().get_option("prefer-install")?)
        {
            if input
                .borrow()
                .get_option("prefer-source")?
                .as_bool()
                .unwrap_or(false)
            {
                return Err(InvalidArgumentException {
                    message: "--prefer-source can not be used together with --prefer-install"
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            if input
                .borrow()
                .get_option("prefer-dist")?
                .as_bool()
                .unwrap_or(false)
            {
                return Err(InvalidArgumentException {
                    message: "--prefer-dist can not be used together with --prefer-install"
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            let prefer_install = input.borrow().get_option("prefer-install")?;
            match prefer_install.as_string().unwrap_or("") {
                "dist" => {
                    input
                        .borrow_mut()
                        .set_option("prefer-dist", PhpMixed::Bool(true))?;
                }
                "source" => {
                    input
                        .borrow_mut()
                        .set_option("prefer-source", PhpMixed::Bool(true))?;
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

        if input
            .borrow()
            .get_option("prefer-source")?
            .as_bool()
            .unwrap_or(false)
            || input
                .borrow()
                .get_option("prefer-dist")?
                .as_bool()
                .unwrap_or(false)
            || (keep_vcs_requires_prefer_source
                && input.borrow().has_option("keep-vcs")
                && input
                    .borrow()
                    .get_option("keep-vcs")?
                    .as_bool()
                    .unwrap_or(false))
        {
            prefer_source = input
                .borrow()
                .get_option("prefer-source")?
                .as_bool()
                .unwrap_or(false)
                || (keep_vcs_requires_prefer_source
                    && input.borrow().has_option("keep-vcs")
                    && input
                        .borrow()
                        .get_option("keep-vcs")?
                        .as_bool()
                        .unwrap_or(false));
            prefer_dist = input
                .borrow()
                .get_option("prefer-dist")?
                .as_bool()
                .unwrap_or(false);
        }

        Ok((prefer_source, prefer_dist))
    }

    fn get_platform_requirement_filter(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
    ) -> Result<Rc<dyn PlatformRequirementFilterInterface>> {
        if !input.borrow().has_option("ignore-platform-reqs")
            || !input.borrow().has_option("ignore-platform-req")
        {
            return Err(LogicException {
                message:
                    "Calling getPlatformRequirementFilter from a command which does not define the --ignore-platform-req[s] flags is not permitted."
                        .to_string(),
                code: 0,
            }
            .into());
        }

        if input
            .borrow()
            .get_option("ignore-platform-reqs")?
            .as_bool()
            .unwrap_or(false)
        {
            return Ok(PlatformRequirementFilterFactory::ignore_all());
        }

        let ignores = input.borrow().get_option("ignore-platform-req")?;
        if count(&ignores) > 0 {
            return PlatformRequirementFilterFactory::from_bool_or_list(ignores);
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
        let parser: VersionParser = VersionParser::new();

        parser.parse_name_version_pairs(requirements)
    }

    fn render_table(&self, table: Vec<PhpMixed>, output: Rc<RefCell<dyn OutputInterface>>) {
        let mut renderer = Table::new(output);
        renderer
            .set_style("compact".into())
            .expect("Table::set_style I/O cannot fail")
            .expect("'compact' is a built-in table style");
        renderer
            .set_rows(table.into_iter().map(Into::into).collect())
            .render();
        let _ = TableSeparator::new();
    }

    fn get_terminal_width(&self) -> i64 {
        let terminal = Terminal::new();
        let mut width = terminal.get_width();

        if Platform::is_windows() {
            width -= 1;
        } else {
            width = width.max(80);
        }

        width
    }

    fn get_audit_format(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        opt_name: &str,
    ) -> Result<String> {
        if !input.borrow().has_option(opt_name) {
            return Err(LogicException {
                message: format!(
                    "This should not be called on a Command which has no {} option defined.",
                    opt_name
                ),
                code: 0,
            }
            .into());
        }

        let val = input.borrow().get_option(opt_name)?;
        let formats: Vec<PhpMixed> = Auditor::FORMATS
            .iter()
            .map(|s| PhpMixed::String(s.to_string()))
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
        input: Rc<RefCell<dyn InputInterface>>,
    ) -> Result<AuditConfig> {
        // Handle both --audit and --no-audit flags
        let audit = if input.borrow().has_option("audit") {
            input
                .borrow()
                .get_option("audit")?
                .as_bool()
                .unwrap_or(false)
        } else {
            !(input.borrow().has_option("no-audit")
                && input
                    .borrow()
                    .get_option("no-audit")?
                    .as_bool()
                    .unwrap_or(false))
        };
        let audit_format = if input.borrow().has_option("audit-format") {
            self.get_audit_format(input.clone(), "audit-format")?
        } else {
            Auditor::FORMAT_SUMMARY.to_string()
        };

        let audit_config = AuditConfig::from_config(config, audit, &audit_format)?;

        if Platform::get_env("COMPOSER_NO_SECURITY_BLOCKING")
            .is_some_and(|s| !s.is_empty() && s != "0")
            || (input.borrow().has_option("no-security-blocking")
                && input
                    .borrow()
                    .get_option("no-security-blocking")?
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

/// \Composer\Command\BaseCommand::initialize — runs for every Composer command after the
/// input is bound. Shared via a free function because Rust has no inheritance; each command's
/// `Command::initialize` forwards here so the leaf's `is_self_update_command()` override (and
/// future overrides) bind correctly.
pub fn base_command_initialize(
    cmd: &dyn BaseCommand,
    input: Rc<RefCell<dyn InputInterface>>,
    _output: Rc<RefCell<dyn OutputInterface>>,
) -> Result<()> {
    // initialize a plugin-enabled Composer instance, either local or global
    // PHP also ORs in $this->getApplication()->getDisablePluginsByDefault() /
    // getDisableScriptsByDefault().
    // TODO(phase-c): the application-default OR-terms need a shared Application handle
    // (deferred), so only the input flags are honoured here.
    let mut disable_plugins = input
        .borrow()
        .has_parameter_option(PhpMixed::from(vec!["--no-plugins"]), false);
    let mut disable_scripts = input
        .borrow()
        .has_parameter_option(PhpMixed::from(vec!["--no-scripts"]), false);

    if cmd.is_self_update_command() {
        disable_plugins = true;
        disable_scripts = true;
    }

    let composer = cmd.try_composer(Some(disable_plugins), Some(disable_scripts));
    let io = cmd.get_io();

    let disable_plugins_kind = if disable_plugins {
        crate::factory::DisablePlugins::All
    } else {
        crate::factory::DisablePlugins::None
    };
    let composer = if composer.is_none() {
        Factory::create_global(io.clone(), disable_plugins_kind, disable_scripts)
    } else {
        composer
    };
    if let Some(composer) = composer.as_ref() {
        let command_name = cmd.get_name().unwrap_or_default();
        let mut pre_command_run_event = PreCommandRunEvent::new(
            PluginEvents::PRE_COMMAND_RUN.to_string(),
            input.clone(),
            command_name,
        );
        let pre_command_run_event_name = pre_command_run_event.get_name().to_string();
        let dispatcher = composer.borrow_partial().get_event_dispatcher();
        dispatcher.borrow_mut().dispatch(
            Some(&pre_command_run_event_name),
            Some(&mut pre_command_run_event),
        )?;
    }

    if input
        .borrow()
        .has_parameter_option(PhpMixed::from(vec!["--no-ansi"]), false)
        && input.borrow().has_option("no-progress")
    {
        input
            .borrow_mut()
            .set_option("no-progress", PhpMixed::Bool(true));
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
            if input.borrow().has_option(option_name)
                && !input
                    .borrow()
                    .get_option(option_name)?
                    .as_bool()
                    .unwrap_or(false)
                && Platform::get_env(env_name).is_some_and(|s| !s.is_empty() && s != "0")
            {
                input
                    .borrow_mut()
                    .set_option(option_name, PhpMixed::Bool(true));
            }
        }
    }

    if input.borrow().has_option("ignore-platform-reqs")
        && !input
            .borrow()
            .get_option("ignore-platform-reqs")?
            .as_bool()
            .unwrap_or(false)
        && Platform::get_env("COMPOSER_IGNORE_PLATFORM_REQS")
            .is_some_and(|s| !s.is_empty() && s != "0")
    {
        input
            .borrow_mut()
            .set_option("ignore-platform-reqs", PhpMixed::Bool(true));

        io.write_error("<warning>COMPOSER_IGNORE_PLATFORM_REQS is set. You may experience unexpected errors.</warning>");
    }

    if input.borrow().has_option("ignore-platform-req")
        && (!input.borrow().has_option("ignore-platform-reqs")
            || !input
                .borrow()
                .get_option("ignore-platform-reqs")?
                .as_bool()
                .unwrap_or(false))
    {
        let ignore_platform_req_env = Platform::get_env("COMPOSER_IGNORE_PLATFORM_REQ");
        let ignore_str = ignore_platform_req_env.clone().unwrap_or_default();
        if 0 == count(&input.borrow().get_option("ignore-platform-req")?)
            && ignore_platform_req_env.is_some()
            && !ignore_str.is_empty()
        {
            input.borrow_mut().set_option(
                "ignore-platform-req",
                PhpMixed::List(
                    explode(",", &ignore_str)
                        .into_iter()
                        .map(PhpMixed::String)
                        .collect(),
                ),
            );

            io.write_error(&format!(
                "<warning>COMPOSER_IGNORE_PLATFORM_REQ is set to ignore {}. You may experience unexpected errors.</warning>",
                ignore_str
            ));
        }
    }

    Ok(())
}
