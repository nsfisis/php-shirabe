//! ref: composer/src/Composer/Plugin/PluginInterface.php

use crate::composer::Composer;
use crate::io::io_interface::IOInterface;

pub const PLUGIN_API_VERSION: &'static str = "2.9.0";

pub trait PluginInterface {
    fn activate(&mut self, composer: &Composer, io: &dyn IOInterface);

    fn deactivate(&mut self, composer: &Composer, io: &dyn IOInterface);

    fn uninstall(&mut self, composer: &Composer, io: &dyn IOInterface);
}
