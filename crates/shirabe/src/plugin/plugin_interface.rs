//! ref: composer/src/Composer/Plugin/PluginInterface.php

use crate::composer::Composer;
use crate::io::io_interface::IOInterface;

pub trait PluginInterface {
    const PLUGIN_API_VERSION: &'static str = "2.9.0";

    fn activate(&mut self, composer: &Composer, io: &dyn IOInterface);

    fn deactivate(&mut self, composer: &Composer, io: &dyn IOInterface);

    fn uninstall(&mut self, composer: &Composer, io: &dyn IOInterface);
}
