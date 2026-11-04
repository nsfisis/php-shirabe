//! ref: composer/vendor/symfony/console/Descriptor/ApplicationDescription.php

use crate::application::Application;
use crate::command::Command;
use crate::exception::CommandNotFoundException;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, bconcat, php_str};

#[derive(Debug)]
pub struct ApplicationDescription {
    application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
    namespace: Option<BString>,
    show_hidden: bool,
    /// Each namespace entry is `['id' => string, 'commands' => string[]]`.
    namespaces: Option<IndexMap<BString, IndexMap<BString, PhpMixed>>>,
    commands: Option<IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>>>,
    aliases: Option<IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>>>,
}

impl ApplicationDescription {
    pub const GLOBAL_NAMESPACE: &'static BStr = php_str(b"_global");

    pub fn new(
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        namespace: Option<BString>,
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

    pub fn get_namespaces(
        &mut self,
    ) -> anyhow::Result<IndexMap<BString, IndexMap<BString, PhpMixed>>> {
        if self.namespaces.is_none() {
            self.inspect_application()?;
        }

        Ok(self.namespaces.clone().unwrap())
    }

    pub fn get_commands(
        &mut self,
    ) -> anyhow::Result<&IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>>> {
        if self.commands.is_none() {
            self.inspect_application()?;
        }

        Ok(self.commands.as_ref().unwrap())
    }

    pub fn get_command(
        &self,
        name: &BStr,
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
                bconcat!("Command \"", name, "\" does not exist."),
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

    fn inspect_application(&mut self) -> anyhow::Result<()> {
        self.commands = Some(IndexMap::new());
        self.namespaces = Some(IndexMap::new());

        let namespace_filter = match &self.namespace {
            Some(ns) if !ns.is_empty() => {
                Some(self.application.borrow_mut().find_namespace(ns.as_bstr())?)
            }
            _ => None,
        };
        let all = self
            .application
            .borrow_mut()
            .all(namespace_filter.as_ref().map(|ns| ns.as_bstr()))?;
        for (namespace, commands) in self.sort_commands(all) {
            let mut names: Vec<BString> = vec![];

            for (name, command) in commands {
                let command_name = command.borrow().get_name();
                let is_hidden = command.borrow().is_hidden();
                if command_name.is_none()
                    || command_name.as_ref().is_some_and(|name| name.is_empty())
                    || (!self.show_hidden && is_hidden)
                {
                    continue;
                }

                if command_name.as_ref() == Some(&name) {
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

            let mut entry: IndexMap<BString, PhpMixed> = IndexMap::new();
            entry.insert("id".into(), PhpMixed::string(namespace.clone()));
            entry.insert(
                "commands".into(),
                PhpMixed::List(names.into_iter().map(PhpMixed::string).collect()),
            );
            self.namespaces.as_mut().unwrap().insert(namespace, entry);
        }

        Ok(())
    }

    fn sort_commands(
        &self,
        commands: IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>>,
    ) -> IndexMap<BString, IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>>> {
        let mut namespaced_commands: IndexMap<
            BString,
            IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>>,
        > = IndexMap::new();
        let mut global_commands: IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>> =
            IndexMap::new();
        let mut sorted_commands: IndexMap<
            BString,
            IndexMap<BString, std::rc::Rc<std::cell::RefCell<dyn Command>>>,
        > = IndexMap::new();
        for (name, command) in commands {
            let key = self
                .application
                .borrow()
                .extract_namespace(name.as_bstr(), Some(1));
            if key.is_empty() || key == Self::GLOBAL_NAMESPACE {
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
            sorted_commands.insert(Self::GLOBAL_NAMESPACE.into(), global_commands);
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
