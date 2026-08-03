//! ref: composer/src/Composer/Plugin/Capability/CommandProvider.php

use crate::command::BaseCommand;
use crate::plugin::capability::Capability;

/// Commands Provider Interface. Plugins implementing this capability provide a list of
/// commands.
///
/// The sole implementor is the PHP capability proxy (Composer itself never implements a
/// capability), so the method is fallible: the answer crosses the RPC boundary.
pub trait CommandProvider: Capability {
    fn get_commands(&self) -> anyhow::Result<Vec<Box<dyn BaseCommand>>>;
}
