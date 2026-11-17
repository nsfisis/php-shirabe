//! The methods a `Composer\Repository\RepositoryManager` entity answers over RPC.

use crate::php_worker::rpc::entity::{Bindings, Call, Entity, entity_value};
use crate::php_worker::rpc::forwarding;
use crate::repository::RepositoryManagerInterface;
use shirabe_php_rpc::{PhpThrow, RpcValue};

impl Entity for std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>> {
    const EXPECTED: &'static [u8] = b"a RepositoryManager";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Bindings<Self>, PhpThrow> {
        Ok(&forwarding::repository::repository_manager::BINDINGS)
    }
}

// TODO(plugin): the remaining RepositoryManager surface is widened on demand.
impl forwarding::repository::repository_manager::Forwarding
    for forwarding::repository::repository_manager::PhpClass
{
    type Handle = std::rc::Rc<std::cell::RefCell<dyn RepositoryManagerInterface>>;

    fn get_local_repository(rm: &Self::Handle, _: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        entity_value(&rm.borrow().get_local_repository())
    }
}
