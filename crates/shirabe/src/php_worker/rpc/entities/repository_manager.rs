//! The methods a `Composer\Repository\RepositoryManager` entity answers over RPC.

use crate::php_worker::rpc::entity::{Call, Crossing, Entity, entity_value};
use crate::php_worker::rpc::forwarding;
use crate::repository::RepositoryManagerInterface;
use shirabe_php_rpc::{PhpThrow, PluginValue};

impl Entity for std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>> {
    const EXPECTED: &'static [u8] = b"a RepositoryManager";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(&forwarding::repository::repository_manager::CROSSING)
    }
}

// TODO(plugin): the remaining RepositoryManager surface is widened on demand.
impl forwarding::repository::repository_manager::Forwarding
    for forwarding::repository::repository_manager::PhpClass
{
    type Handle = std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>;

    fn get_local_repository(rm: &Self::Handle, _: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        entity_value(&rm.borrow().get_local_repository())
    }
}
