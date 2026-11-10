//! The methods a `Composer\DependencyResolver\Transaction` entity answers over RPC.

use crate::dependency_resolver::AnyTransaction;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity, entity_value};
use crate::php_worker::rpc::forwarding;
use shirabe_php_rpc::{PhpThrow, PluginValue};

impl Entity for std::rc::Rc<AnyTransaction> {
    const EXPECTED: &'static [u8] = b"a Transaction";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(match &**self {
            AnyTransaction::Transaction(_) => {
                &forwarding::dependency_resolver::transaction::CROSSING
            }
            AnyTransaction::LocalRepoTransaction(_) => {
                &forwarding::dependency_resolver::local_repo_transaction::CROSSING
            }
        })
    }
}

impl forwarding::dependency_resolver::transaction::Forwarding
    for forwarding::dependency_resolver::transaction::PhpClass
{
    type Handle = std::rc::Rc<AnyTransaction>;

    fn get_operations(
        transaction: &Self::Handle,
        _: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(PluginValue::List(
            transaction
                .get_operations()
                .iter()
                .map(entity_value)
                .collect::<Result<_, _>>()?,
        ))
    }
}

impl forwarding::dependency_resolver::local_repo_transaction::Forwarding
    for forwarding::dependency_resolver::local_repo_transaction::PhpClass
{
}
