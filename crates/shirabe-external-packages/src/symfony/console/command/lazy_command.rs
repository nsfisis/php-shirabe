use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::{BaseCommand, Command, SetDefinitionArg};
use crate::symfony::console::completion::completion_input::CompletionInput;
use crate::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;

/// Either an already-built command, or a factory closure that builds one.
///
/// PHP: `private $command` holds a `Command` instance or a `\Closure`.
pub enum LazyCommandInner {
    Command(Box<dyn Command>),
    Factory(Box<dyn Fn() -> Box<dyn Command>>),
}

impl std::fmt::Debug for LazyCommandInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LazyCommandInner::Command(command) => f.debug_tuple("Command").field(command).finish(),
            LazyCommandInner::Factory(_) => f.debug_tuple("Factory").finish(),
        }
    }
}

#[derive(Debug)]
pub struct LazyCommand {
    inner: BaseCommand,
    command: LazyCommandInner,
    is_enabled: Option<bool>,
}

impl Deref for LazyCommand {
    type Target = BaseCommand;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for LazyCommand {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl LazyCommand {
    pub fn new(
        name: &str,
        aliases: Vec<String>,
        description: &str,
        is_hidden: bool,
        command_factory: Box<dyn Fn() -> Box<dyn Command>>,
        is_enabled: Option<bool>,
    ) -> anyhow::Result<Self> {
        let mut this = Self {
            inner: BaseCommand::__construct(None)?,
            command: LazyCommandInner::Factory(command_factory),
            is_enabled,
        };

        this.inner
            .set_name(name)?
            .set_aliases(aliases)?
            .set_hidden(is_hidden)
            .set_description(description);

        Ok(this)
    }

    pub fn ignore_validation_errors(&mut self) {
        self.get_command().ignore_validation_errors();
    }

    pub fn set_application(&mut self, application: Option<Rc<RefCell<Application>>>) {
        // if ($this->command instanceof parent)
        if let LazyCommandInner::Command(command) = &mut self.command {
            command.set_application(application.clone());
        }

        // parent::setApplication($application);
        self.inner.set_application(application);
    }

    pub fn set_helper_set(&mut self, helper_set: Rc<RefCell<HelperSet>>) {
        // if ($this->command instanceof parent)
        if let LazyCommandInner::Command(command) = &mut self.command {
            command.set_helper_set(helper_set.clone());
        }

        // parent::setHelperSet($helperSet);
        self.inner.set_helper_set(helper_set);
    }

    pub fn is_enabled(&mut self) -> bool {
        // $this->isEnabled ?? $this->getCommand()->isEnabled()
        match self.is_enabled {
            Some(is_enabled) => is_enabled,
            None => self.get_command().is_enabled(),
        }
    }

    pub fn run(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        self.get_command().run(input, output)
    }

    pub fn complete(&mut self, input: &CompletionInput, suggestions: &mut CompletionSuggestions) {
        self.get_command().complete(input, suggestions);
    }

    pub fn set_code(
        &mut self,
        code: Box<dyn Fn(&mut dyn InputInterface, &mut dyn OutputInterface) -> PhpMixed>,
    ) -> &mut Self {
        // TODO: Command::set_code() lives on BaseCommand's inherent API and is not part of
        // the polymorphic `Command` trait, so it cannot be forwarded through `get_command()`
        // (a `&mut Box<dyn Command>`). Resolving this needs `set_code` on the trait (Phase C).
        let _ = code;
        todo!()
    }

    /// @internal
    pub fn merge_application_definition(&mut self, merge_args: bool) {
        self.get_command().merge_application_definition(merge_args);
    }

    pub fn set_definition(&mut self, definition: SetDefinitionArg) -> &mut Self {
        // TODO: Command::set_definition() is not part of the polymorphic `Command` trait;
        // it cannot be forwarded through `get_command()` (Phase C).
        let _ = definition;
        todo!()
    }

    pub fn get_definition(&mut self) -> &InputDefinition {
        self.get_command().get_definition()
    }

    pub fn get_native_definition(&mut self) -> &InputDefinition {
        self.get_command().get_native_definition()
    }

    pub fn add_argument(
        &mut self,
        name: &str,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> &mut Self {
        // TODO: Command::add_argument() is not part of the polymorphic `Command` trait;
        // it cannot be forwarded through `get_command()` (Phase C).
        let _ = (name, mode, description, default);
        todo!()
    }

    pub fn add_option(
        &mut self,
        name: &str,
        shortcut: PhpMixed,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> &mut Self {
        // TODO: Command::add_option() is not part of the polymorphic `Command` trait;
        // it cannot be forwarded through `get_command()` (Phase C).
        let _ = (name, shortcut, mode, description, default);
        todo!()
    }

    pub fn set_process_title(&mut self, title: &str) -> &mut Self {
        // TODO: Command::set_process_title() is not part of the polymorphic `Command` trait;
        // it cannot be forwarded through `get_command()` (Phase C).
        let _ = title;
        todo!()
    }

    pub fn set_help(&mut self, help: &str) -> &mut Self {
        self.get_command().set_help(help);

        self
    }

    pub fn get_help(&mut self) -> String {
        self.get_command().get_help()
    }

    pub fn get_processed_help(&mut self) -> String {
        self.get_command().get_processed_help()
    }

    pub fn get_synopsis(&mut self, short: bool) -> String {
        self.get_command().get_synopsis(short)
    }

    pub fn add_usage(&mut self, usage: &str) -> &mut Self {
        // TODO: Command::add_usage() is not part of the polymorphic `Command` trait;
        // it cannot be forwarded through `get_command()` (Phase C).
        let _ = usage;
        todo!()
    }

    pub fn get_usages(&mut self) -> Vec<String> {
        self.get_command().get_usages()
    }

    pub fn get_helper(
        &mut self,
        name: &str,
    ) -> anyhow::Result<
        Result<PhpMixed, crate::symfony::console::exception::logic_exception::LogicException>,
    > {
        self.get_command().get_helper(name)
    }

    pub fn get_command(&mut self) -> &mut Box<dyn Command> {
        // if (!$this->command instanceof \Closure) { return $this->command; }
        if let LazyCommandInner::Command(_) = &self.command {
            if let LazyCommandInner::Command(command) = &mut self.command {
                return command;
            }
            unreachable!()
        }

        // $command = $this->command = ($this->command)();
        let mut command = match &self.command {
            LazyCommandInner::Factory(factory) => factory(),
            LazyCommandInner::Command(_) => unreachable!(),
        };
        command.set_application(self.inner.get_application());

        // if (null !== $this->getHelperSet())
        if let Some(helper_set) = self.inner.get_helper_set() {
            command.set_helper_set(helper_set);
        }

        let name = self.inner.get_name().unwrap_or_default();
        let aliases = self.inner.get_aliases();
        let hidden = self.inner.is_hidden();
        let description = self.inner.get_description();
        let _ = command.set_name(&name);
        let _ = command.set_aliases(aliases);
        command.set_hidden(hidden);
        command.set_description(&description);

        // Will throw if the command is not correctly initialized.
        command.get_definition();

        self.command = LazyCommandInner::Command(command);
        match &mut self.command {
            LazyCommandInner::Command(command) => command,
            LazyCommandInner::Factory(_) => unreachable!(),
        }
    }
}

impl Command for LazyCommand {
    fn configure(&mut self) {
        // LazyCommand has no configure() of its own; nothing to do.
    }

    fn run(
        &mut self,
        input: &mut dyn InputInterface,
        output: &mut dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        LazyCommand::run(self, input, output)
    }

    fn complete(&self, _input: &CompletionInput, _suggestions: &mut CompletionSuggestions) {
        // TODO: LazyCommand::complete() lazily materializes the wrapped command and so needs
        // `&mut self`, which conflicts with the `Command::complete(&self, ...)` signature
        // (Phase C).
        todo!()
    }

    fn is_enabled(&self) -> bool {
        // TODO: LazyCommand::is_enabled() lazily materializes the wrapped command and so needs
        // `&mut self`, which conflicts with the trait signature (Phase C).
        todo!()
    }

    fn set_application(&mut self, application: Option<Rc<RefCell<Application>>>) {
        LazyCommand::set_application(self, application);
    }

    fn get_application(&self) -> Option<Rc<RefCell<Application>>> {
        self.inner.get_application()
    }

    fn set_helper_set(&mut self, helper_set: Rc<RefCell<HelperSet>>) {
        LazyCommand::set_helper_set(self, helper_set);
    }

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn merge_application_definition(&mut self, merge_args: bool) {
        LazyCommand::merge_application_definition(self, merge_args);
    }

    fn get_definition(&self) -> &InputDefinition {
        // TODO: LazyCommand::get_definition() lazily materializes the wrapped command and so
        // needs `&mut self`, which conflicts with the trait signature (Phase C).
        todo!()
    }

    fn get_native_definition(&self) -> &InputDefinition {
        // TODO: same lazy-materialization / `&self` conflict as get_definition() (Phase C).
        todo!()
    }

    fn set_name(&mut self, name: &str) -> anyhow::Result<()> {
        self.inner.set_name(name)?;
        Ok(())
    }

    fn get_name(&self) -> Option<String> {
        self.inner.get_name()
    }

    fn set_hidden(&mut self, hidden: bool) {
        self.inner.set_hidden(hidden);
    }

    fn is_hidden(&self) -> bool {
        self.inner.is_hidden()
    }

    fn set_description(&mut self, description: &str) {
        self.inner.set_description(description);
    }

    fn get_description(&self) -> String {
        self.inner.get_description()
    }

    fn set_help(&mut self, help: &str) {
        LazyCommand::set_help(self, help);
    }

    fn get_help(&self) -> String {
        // TODO: LazyCommand::get_help() lazily materializes the wrapped command and so needs
        // `&mut self`, which conflicts with the trait signature (Phase C).
        todo!()
    }

    fn get_processed_help(&self) -> String {
        // TODO: same lazy-materialization / `&self` conflict (Phase C).
        todo!()
    }

    fn set_aliases(&mut self, aliases: Vec<String>) -> anyhow::Result<()> {
        self.inner.set_aliases(aliases)?;
        Ok(())
    }

    fn get_aliases(&self) -> Vec<String> {
        self.inner.get_aliases()
    }

    fn get_synopsis(&mut self, short: bool) -> String {
        LazyCommand::get_synopsis(self, short)
    }

    fn get_usages(&self) -> Vec<String> {
        // TODO: LazyCommand::get_usages() lazily materializes the wrapped command and so needs
        // `&mut self`, which conflicts with the trait signature (Phase C).
        todo!()
    }

    fn get_helper(
        &self,
        _name: &str,
    ) -> anyhow::Result<
        Result<PhpMixed, crate::symfony::console::exception::logic_exception::LogicException>,
    > {
        // TODO: LazyCommand::get_helper() lazily materializes the wrapped command and so needs
        // `&mut self`, which conflicts with the trait signature (Phase C).
        todo!()
    }

    fn ignore_validation_errors(&mut self) {
        LazyCommand::ignore_validation_errors(self);
    }
}
