//! The methods the events declare, answered over RPC: the base event's, and those each event
//! class declares beyond it.

use crate::composer::ComposerHandle;
use crate::event_dispatcher::{Event, EventInterface};
use crate::installer::{InstallerEvent, PackageEvent};
use crate::package::PackageInterfaceHandle;
use crate::php_worker::rpc::entity::{Bindings, Call, Entity, entity_value};
use crate::php_worker::rpc::forwarding;
use crate::plugin::{
    CommandEvent, FileDownloadContext, PostFileDownloadEvent, PreCommandRunEvent,
    PreFileDownloadEvent, PrePoolCreateEvent,
};
use crate::script::Event as ScriptEvent;
use crate::util::StreamContextOptions;
use bstr::BString;
use shirabe_php_rpc::{PhpThrow, RpcValue, ToRpcValue};
use shirabe_php_shim::{PhpMixed, bformat};

impl Entity for std::rc::Rc<std::cell::RefCell<dyn EventInterface>> {
    const EXPECTED: &'static [u8] = b"an event";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Bindings<Self>, PhpThrow> {
        let event = self.borrow();
        let any = event.as_any();
        if any.is::<ScriptEvent>() {
            Ok(&forwarding::script::event::BINDINGS)
        } else if any.is::<PackageEvent>() {
            Ok(&forwarding::installer::package_event::BINDINGS)
        } else if any.is::<InstallerEvent>() {
            Ok(&forwarding::installer::installer_event::BINDINGS)
        } else if any.is::<PreFileDownloadEvent>() {
            Ok(&forwarding::plugin::pre_file_download_event::BINDINGS)
        } else if any.is::<PostFileDownloadEvent>() {
            Ok(&forwarding::plugin::post_file_download_event::BINDINGS)
        } else if any.is::<PrePoolCreateEvent>() {
            Ok(&forwarding::plugin::pre_pool_create_event::BINDINGS)
        } else if any.is::<CommandEvent>() {
            Ok(&forwarding::plugin::command_event::BINDINGS)
        } else if any.is::<PreCommandRunEvent>() {
            Ok(&forwarding::plugin::pre_command_run_event::BINDINGS)
        } else if any.is::<Event>() {
            Ok(&forwarding::event_dispatcher::event::BINDINGS)
        } else {
            // TODO(plugin): an event class without a proxy stub cannot cross the boundary.
            Err(PhpThrow::runtime(bformat!(
                "no proxy stub is available yet for the event `{}`",
                event.get_name()
            )))
        }
    }
}

/// The concrete event behind `event`, which crosses as the class `T` stands for.
fn concrete<T: 'static>(
    event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
) -> std::cell::RefMut<'_, T> {
    std::cell::RefMut::map(event.borrow_mut(), |event| {
        event
            .as_any_mut()
            .downcast_mut::<T>()
            .expect("an event crosses as the class of its concrete type")
    })
}

impl forwarding::event_dispatcher::event::Forwarding
    for forwarding::event_dispatcher::event::PhpClass
{
    type Handle = std::rc::Rc<std::cell::RefCell<dyn EventInterface>>;

    fn get_name(event: &Self::Handle, _: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::string(event.borrow().get_name()))
    }

    fn get_arguments(event: &Self::Handle, _: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::List(
            event
                .borrow()
                .get_arguments()
                .iter()
                .map(|arg| RpcValue::string(arg.clone()))
                .collect(),
        ))
    }

    fn get_flags(event: &Self::Handle, _: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        Ok(PhpMixed::Array(event.borrow().get_flags().to_php_array()).to_rpc_value())
    }

    fn is_propagation_stopped(
        event: &Self::Handle,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::Bool(event.borrow().is_propagation_stopped()))
    }

    fn stop_propagation(event: &Self::Handle, _: &mut Call<'_>) -> Result<RpcValue, PhpThrow> {
        event.borrow_mut().stop_propagation();
        Ok(RpcValue::Null)
    }
}

/// The `$composer` of a script, package or installer event.
fn composer_value(composer: Option<ComposerHandle>) -> Result<RpcValue, PhpThrow> {
    let composer =
        composer.ok_or_else(|| PhpThrow::runtime("the Composer instance of this event is gone"))?;
    entity_value(&composer)
}

// TODO(plugin): the originating event of a script event needs the event it wraps proxied the
// other way round.
impl forwarding::script::event::Forwarding for forwarding::script::event::PhpClass {
    fn get_composer(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let composer = concrete::<ScriptEvent>(event).get_composer().upgrade();
        composer_value(composer)
    }

    fn get_io(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let io = concrete::<ScriptEvent>(event).get_io();
        entity_value(&io)
    }

    fn is_dev_mode(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::Bool(concrete::<ScriptEvent>(event).is_dev_mode()))
    }
}

impl forwarding::installer::package_event::Forwarding
    for forwarding::installer::package_event::PhpClass
{
    fn get_composer(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let composer = concrete::<PackageEvent>(event).get_composer().upgrade();
        composer_value(composer)
    }

    fn get_io(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let io = concrete::<PackageEvent>(event).get_io();
        entity_value(&io)
    }

    fn is_dev_mode(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::Bool(
            concrete::<PackageEvent>(event).is_dev_mode(),
        ))
    }

    fn get_local_repo(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let repository = concrete::<PackageEvent>(event)
            .get_local_repo()
            .as_repository_handle();
        entity_value(&repository)
    }

    fn get_operations(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let operations = concrete::<PackageEvent>(event).get_operations().to_vec();
        Ok(RpcValue::List(
            operations
                .iter()
                .map(entity_value)
                .collect::<Result<_, _>>()?,
        ))
    }

    fn get_operation(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let operation = concrete::<PackageEvent>(event).get_operation().clone();
        entity_value(&operation)
    }
}

impl forwarding::installer::installer_event::Forwarding
    for forwarding::installer::installer_event::PhpClass
{
    fn get_composer(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let composer = concrete::<InstallerEvent>(event).get_composer().upgrade();
        composer_value(composer)
    }

    fn get_io(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let io = concrete::<InstallerEvent>(event).get_io();
        entity_value(&io)
    }

    fn is_dev_mode(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::Bool(
            concrete::<InstallerEvent>(event).is_dev_mode(),
        ))
    }

    fn is_executing_operations(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::Bool(
            concrete::<InstallerEvent>(event).is_executing_operations(),
        ))
    }

    fn get_transaction(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let transaction = concrete::<InstallerEvent>(event).get_transaction().cloned();
        match transaction {
            Some(transaction) => entity_value(&transaction),
            None => Ok(RpcValue::Null),
        }
    }
}

impl forwarding::plugin::command_event::Forwarding for forwarding::plugin::command_event::PhpClass {
    fn get_command_name(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::string(
            concrete::<CommandEvent>(event).get_command_name(),
        ))
    }
}

impl forwarding::plugin::pre_command_run_event::Forwarding
    for forwarding::plugin::pre_command_run_event::PhpClass
{
    fn get_command(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(RpcValue::string(
            concrete::<PreCommandRunEvent>(event).get_command(),
        ))
    }
}

/// The `$context` of a file download event as the PHP value the event stub returns.
fn file_download_context_value(
    context: Option<&FileDownloadContext>,
) -> Result<RpcValue, PhpThrow> {
    match context {
        None => Ok(RpcValue::Null),
        Some(FileDownloadContext::Package(package)) => Ok(package.clone().to_rpc_value()),
        // TODO(plugin): the array a repository passes names the repository itself, which cannot
        // cross the boundary yet.
        Some(FileDownloadContext::Repository) => Err(PhpThrow::runtime(
            "the repository context of a file download event is not available over RPC yet",
        )),
    }
}

impl forwarding::plugin::pre_file_download_event::Forwarding
    for forwarding::plugin::pre_file_download_event::PhpClass
{
    fn get_http_downloader(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let downloader = concrete::<PreFileDownloadEvent>(event)
            .get_http_downloader()
            .clone();
        entity_value(&downloader)
    }

    fn get_processed_url(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PreFileDownloadEvent>(event)
            .get_processed_url()
            .to_rpc_value())
    }

    fn set_processed_url(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let url = call.arg::<BString>(0)?;
        concrete::<PreFileDownloadEvent>(event).set_processed_url(url);
        Ok(RpcValue::Null)
    }

    fn get_custom_cache_key(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PreFileDownloadEvent>(event)
            .get_custom_cache_key()
            .to_rpc_value())
    }

    fn set_custom_cache_key(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let key = call.arg::<Option<BString>>(0)?;
        concrete::<PreFileDownloadEvent>(event).set_custom_cache_key(key);
        Ok(RpcValue::Null)
    }

    fn get_type(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PreFileDownloadEvent>(event)
            .get_type()
            .to_rpc_value())
    }

    fn get_context(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        file_download_context_value(concrete::<PreFileDownloadEvent>(event).get_context())
    }

    fn get_transport_options(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PreFileDownloadEvent>(event)
            .get_transport_options()
            .clone()
            .to_rpc_value())
    }

    fn set_transport_options(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let options = call.arg::<StreamContextOptions>(0)?;
        concrete::<PreFileDownloadEvent>(event).set_transport_options(options);
        Ok(RpcValue::Null)
    }
}

impl forwarding::plugin::post_file_download_event::Forwarding
    for forwarding::plugin::post_file_download_event::PhpClass
{
    fn get_file_name(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PostFileDownloadEvent>(event)
            .get_file_name()
            .to_rpc_value())
    }

    fn get_checksum(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PostFileDownloadEvent>(event)
            .get_checksum()
            .to_rpc_value())
    }

    fn get_url(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PostFileDownloadEvent>(event)
            .get_url()
            .to_rpc_value())
    }

    fn get_context(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        file_download_context_value(concrete::<PostFileDownloadEvent>(event).get_context())
    }

    fn get_package(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PostFileDownloadEvent>(event)
            .get_package()
            .to_rpc_value())
    }

    fn get_type(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PostFileDownloadEvent>(event)
            .get_type()
            .to_rpc_value())
    }
}

impl forwarding::plugin::pre_pool_create_event::Forwarding
    for forwarding::plugin::pre_pool_create_event::PhpClass
{
    fn get_repositories(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let repositories = concrete::<PrePoolCreateEvent>(event)
            .get_repositories()
            .to_vec();
        Ok(RpcValue::List(
            repositories
                .iter()
                .map(entity_value)
                .collect::<Result<_, _>>()?,
        ))
    }

    fn get_acceptable_stabilities(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PrePoolCreateEvent>(event)
            .get_acceptable_stabilities()
            .clone()
            .to_rpc_value())
    }

    fn get_stability_flags(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PrePoolCreateEvent>(event)
            .get_stability_flags()
            .clone()
            .to_rpc_value())
    }

    fn get_root_aliases(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PrePoolCreateEvent>(event)
            .get_root_aliases()
            .clone()
            .to_rpc_value())
    }

    fn get_root_references(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PrePoolCreateEvent>(event)
            .get_root_references()
            .clone()
            .to_rpc_value())
    }

    fn get_packages(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PrePoolCreateEvent>(event)
            .get_packages()
            .clone()
            .to_rpc_value())
    }

    fn get_unacceptable_fixed_packages(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        _: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        Ok(concrete::<PrePoolCreateEvent>(event)
            .get_unacceptable_fixed_packages()
            .clone()
            .to_rpc_value())
    }

    fn set_packages(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let packages = call.arg::<Vec<PackageInterfaceHandle>>(0)?;
        concrete::<PrePoolCreateEvent>(event).set_packages(packages);
        Ok(RpcValue::Null)
    }

    fn set_unacceptable_fixed_packages(
        event: &std::rc::Rc<std::cell::RefCell<dyn EventInterface>>,
        call: &mut Call<'_>,
    ) -> Result<RpcValue, PhpThrow> {
        let packages = call.arg::<Vec<PackageInterfaceHandle>>(0)?;
        concrete::<PrePoolCreateEvent>(event).set_unacceptable_fixed_packages(packages);
        Ok(RpcValue::Null)
    }
}
