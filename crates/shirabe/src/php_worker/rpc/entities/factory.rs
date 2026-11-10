//! The static methods of `Composer\Factory` answered over RPC.

use crate::factory::Factory;
use crate::php_worker::rpc::entity::Call;
use crate::php_worker::rpc::forwarding;
use shirabe_php_rpc::{PhpThrow, PluginValue};
use shirabe_php_shim::path_to_bytes;

impl forwarding::factory::Forwarding for forwarding::factory::PhpClass {
    fn get_composer_file(call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Factory::get_composer_file()
            .map(|file| PluginValue::string(path_to_bytes(&file)))
            .map_err(call.failed())
    }
}
