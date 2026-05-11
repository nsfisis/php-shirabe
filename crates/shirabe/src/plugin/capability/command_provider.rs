//! ref: composer/src/Composer/Plugin/Capability/CommandProvider.php

// TODO(plugin): Commands Provider Interface. Plugins implementing this capability provide a list of commands.
use crate::command::base_command::BaseCommand;
use crate::plugin::capability::capability::Capability;

pub trait CommandProvider: Capability {
    fn get_commands(&self) -> Vec<Box<BaseCommand>>;
}
