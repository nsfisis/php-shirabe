//! ref: composer/vendor/symfony/console/Descriptor/ApplicationDescription.php

use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::exception::command_not_found_exception::CommandNotFoundException;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// @internal
#[derive(Debug)]
pub struct ApplicationDescription {
    application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
    namespace: Option<String>,
    show_hidden: bool,

    /// @var array
    /// Each namespace entry is `['id' => string, 'commands' => string[]]`.
    namespaces: Option<IndexMap<String, IndexMap<String, PhpMixed>>>,

    /// @var array<string, Command>
    commands: Option<IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn Command>>>>,

    /// @var array<string, Command>
    aliases: Option<IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn Command>>>>,
}

impl ApplicationDescription {
    pub const GLOBAL_NAMESPACE: &'static str = "_global";

    pub fn new(
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        namespace: Option<String>,
        show_hidden: bool,
    ) -> Self {
        ApplicationDescription {
            application,
            namespace,
            show_hidden,
            namespaces: None,
            commands: None,
            aliases: None,
        }
    }

    pub fn get_namespaces(&mut self) -> IndexMap<String, IndexMap<String, PhpMixed>> {
        if self.namespaces.is_none() {
            self.inspect_application();
        }

        self.namespaces.clone().unwrap()
    }

    /// @return Command[]
    pub fn get_commands(
        &mut self,
    ) -> &IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn Command>>> {
        if self.commands.is_none() {
            self.inspect_application();
        }

        self.commands.as_ref().unwrap()
    }

    /// @throws CommandNotFoundException
    pub fn get_command(
        &self,
        name: &str,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<dyn Command>>> {
        let in_commands = self
            .commands
            .as_ref()
            .map(|c| c.contains_key(name))
            .unwrap_or(false);
        let in_aliases = self
            .aliases
            .as_ref()
            .map(|a| a.contains_key(name))
            .unwrap_or(false);
        if !in_commands && !in_aliases {
            return Err(CommandNotFoundException::new(
                format!("Command \"{}\" does not exist.", name),
                vec![],
                0,
            )
            .into());
        }

        Ok(self
            .commands
            .as_ref()
            .and_then(|c| c.get(name))
            .cloned()
            .unwrap_or_else(|| self.aliases.as_ref().unwrap().get(name).unwrap().clone()))
    }

    fn inspect_application(&mut self) {
        self.commands = Some(IndexMap::new());
        self.namespaces = Some(IndexMap::new());

        let namespace_filter = match &self.namespace {
            Some(ns) if !ns.is_empty() => {
                Some(self.application.borrow_mut().find_namespace(ns).unwrap())
            }
            _ => None,
        };
        let all = self
            .application
            .borrow_mut()
            .all(namespace_filter.as_deref())
            .unwrap();
        for (namespace, commands) in self.sort_commands(all) {
            let mut names: Vec<String> = vec![];

            for (name, command) in commands {
                let command_name = command.borrow().get_name();
                let is_hidden = command.borrow().is_hidden();
                if command_name.is_none()
                    || command_name.as_deref() == Some("")
                    || (!self.show_hidden && is_hidden)
                {
                    continue;
                }

                if command_name.as_deref() == Some(name.as_str()) {
                    self.commands
                        .as_mut()
                        .unwrap()
                        .insert(name.clone(), command);
                } else {
                    self.aliases
                        .get_or_insert_with(IndexMap::new)
                        .insert(name.clone(), command);
                }

                names.push(name);
            }

            let mut entry: IndexMap<String, PhpMixed> = IndexMap::new();
            entry.insert("id".to_string(), PhpMixed::String(namespace.clone()));
            entry.insert(
                "commands".to_string(),
                PhpMixed::List(names.into_iter().map(PhpMixed::String).collect()),
            );
            self.namespaces.as_mut().unwrap().insert(namespace, entry);
        }
    }

    fn sort_commands(
        &self,
        commands: IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn Command>>>,
    ) -> IndexMap<String, IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn Command>>>> {
        let mut namespaced_commands: IndexMap<
            String,
            IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn Command>>>,
        > = IndexMap::new();
        let mut global_commands: IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn Command>>> =
            IndexMap::new();
        let mut sorted_commands: IndexMap<
            String,
            IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn Command>>>,
        > = IndexMap::new();
        for (name, command) in commands {
            let key = self.application.borrow().extract_namespace(&name, Some(1));
            if ["", Self::GLOBAL_NAMESPACE].contains(&key.as_str()) {
                global_commands.insert(name, command);
            } else {
                namespaced_commands
                    .entry(key)
                    .or_default()
                    .insert(name, command);
            }
        }

        if !global_commands.is_empty() {
            global_commands.sort_keys();
            sorted_commands.insert(Self::GLOBAL_NAMESPACE.to_string(), global_commands);
        }

        if !namespaced_commands.is_empty() {
            // ksort($namespacedCommands, \SORT_STRING)
            namespaced_commands.sort_keys();
            for (key, mut commands_set) in namespaced_commands {
                commands_set.sort_keys();
                sorted_commands.insert(key, commands_set);
            }
        }

        sorted_commands
    }
}
