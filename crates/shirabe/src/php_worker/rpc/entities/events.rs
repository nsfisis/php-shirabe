//! The methods the events declare, answered over RPC: the base event's, the script, package and
//! installer events', and those the plugin events (`PreFileDownloadEvent`,
//! `PostFileDownloadEvent`, `PrePoolCreateEvent`) declare beyond the base event's.

use crate::event_dispatcher::EventInterface;
use crate::installer::{InstallerEvent, PackageEvent};
use crate::package::PackageInterfaceHandle;
use crate::php_worker::rpc::args::arg;
use crate::php_worker::rpc::dispatch::unavailable_method;
use crate::php_worker::rpc::table::{
    RustEntity, io_stub_class, operation_handle_value, register_composer_entity, register_entity,
    register_io_entity, repository_handle_value, rust_handle_value, transaction_handle_value,
};
use crate::plugin::{
    CommandEvent, FileDownloadContext, PostFileDownloadEvent, PreCommandRunEvent,
    PreFileDownloadEvent, PrePoolCreateEvent,
};
use crate::script::Event as ScriptEvent;
use crate::util::StreamContextOptions;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{PhpMixed, bformat, error_message};

/// Serves the methods `PreFileDownloadEvent` declares beyond the base event's.
pub(crate) fn dispatch_pre_file_download_event_method(
    event: &mut PreFileDownloadEvent,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getHttpDownloader" => Ok(rust_handle_value(
            register_entity(RustEntity::HttpDownloader(
                event.get_http_downloader().clone(),
            )),
            BStr::new("Composer\\Util\\HttpDownloader"),
        )),
        b"getProcessedUrl" => Ok(event.get_processed_url().to_wire()),
        b"setProcessedUrl" => {
            event.set_processed_url(arg::<BString>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        b"getCustomCacheKey" => Ok(event.get_custom_cache_key().to_wire()),
        b"setCustomCacheKey" => {
            event.set_custom_cache_key(arg::<Option<BString>>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        b"getType" => Ok(event.get_type().to_wire()),
        b"getContext" => file_download_context_value(event.get_context()),
        b"getTransportOptions" => Ok(event.get_transport_options().clone().to_wire()),
        b"setTransportOptions" => {
            event.set_transport_options(arg::<StreamContextOptions>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        other => Err(unavailable_method("Event", other)),
    }
}

/// Serves the methods `PostFileDownloadEvent` declares beyond the base event's.
pub(crate) fn dispatch_post_file_download_event_method(
    event: &PostFileDownloadEvent,
    method_name: &BStr,
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getFileName" => Ok(event.get_file_name().to_wire()),
        b"getChecksum" => Ok(event.get_checksum().to_wire()),
        b"getUrl" => Ok(event.get_url().to_wire()),
        b"getContext" => file_download_context_value(event.get_context()),
        b"getPackage" => Ok(event.get_package().to_wire()),
        b"getType" => Ok(event.get_type().to_wire()),
        other => Err(unavailable_method("Event", other)),
    }
}

/// Serves the methods `PrePoolCreateEvent` declares beyond the base event's.
pub(crate) fn dispatch_pre_pool_create_event_method(
    event: &mut PrePoolCreateEvent,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getRepositories" => Ok(PluginValue::List(
            event
                .get_repositories()
                .iter()
                .map(repository_handle_value)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        // TODO(plugin): Request has no proxy stub, so the request a listener would inspect or
        // change cannot cross the boundary yet.
        b"getRequest" => Err(PhpThrow::runtime(
            "the Request of a PrePoolCreateEvent is not available over RPC yet",
        )),
        b"getAcceptableStabilities" => Ok(event.get_acceptable_stabilities().clone().to_wire()),
        b"getStabilityFlags" => Ok(event.get_stability_flags().clone().to_wire()),
        b"getRootAliases" => Ok(event.get_root_aliases().clone().to_wire()),
        b"getRootReferences" => Ok(event.get_root_references().clone().to_wire()),
        b"getPackages" => Ok(event.get_packages().clone().to_wire()),
        b"getUnacceptableFixedPackages" => {
            Ok(event.get_unacceptable_fixed_packages().clone().to_wire())
        }
        b"setPackages" => {
            event.set_packages(arg::<Vec<PackageInterfaceHandle>>(method_name, args, 0)?);
            Ok(PluginValue::Null)
        }
        b"setUnacceptableFixedPackages" => {
            event.set_unacceptable_fixed_packages(arg::<Vec<PackageInterfaceHandle>>(
                method_name,
                args,
                0,
            )?);
            Ok(PluginValue::Null)
        }
        other => Err(unavailable_method("Event", other)),
    }
}

/// The `$context` of a file download event as the PHP value the event stub returns.
fn file_download_context_value(
    context: Option<&FileDownloadContext>,
) -> Result<PluginValue, PhpThrow> {
    match context {
        None => Ok(PluginValue::Null),
        Some(FileDownloadContext::Package(package)) => Ok(package.clone().to_wire()),
        // TODO(plugin): the array a repository passes names the repository itself, which cannot
        // cross the boundary yet.
        Some(FileDownloadContext::Repository) => Err(PhpThrow::runtime(
            "the repository context of a file download event is not available over RPC yet",
        )),
    }
}

/// Serves a method call on an event entity of the R table.
pub(crate) fn dispatch_event_method(
    event: &mut dyn EventInterface,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    match method_name.as_bytes() {
        b"getName" => Ok(PluginValue::string(event.get_name())),
        b"getArguments" => Ok(PluginValue::List(
            event
                .get_arguments()
                .iter()
                .map(|arg| PluginValue::string(arg.clone()))
                .collect(),
        )),
        b"getFlags" => Ok(PhpMixed::Array(event.get_flags().to_php_array()).to_wire()),
        b"isPropagationStopped" => Ok(PluginValue::Bool(event.is_propagation_stopped())),
        b"stopPropagation" => {
            event.stop_propagation();
            Ok(PluginValue::Null)
        }
        b"isDevMode" => match (
            script_event(event),
            package_event(event),
            installer_event(event),
        ) {
            (Some(event), _, _) => Ok(PluginValue::Bool(event.is_dev_mode())),
            (_, Some(event), _) => Ok(PluginValue::Bool(event.is_dev_mode())),
            (_, _, Some(event)) => Ok(PluginValue::Bool(event.is_dev_mode())),
            _ => Err(PhpThrow::runtime(BString::from(
                "isDevMode is only available on script, package and installer events",
            ))),
        },
        b"getComposer" => {
            let composer = match (
                script_event(event),
                package_event(event),
                installer_event(event),
            ) {
                (Some(event), _, _) => event.get_composer().upgrade(),
                (_, Some(event), _) => event.get_composer().upgrade(),
                (_, _, Some(event)) => event.get_composer().upgrade(),
                _ => {
                    return Err(PhpThrow::runtime(BString::from(
                        "getComposer is only available on script, package and installer events",
                    )));
                }
            };
            let composer = composer.ok_or_else(|| {
                PhpThrow::runtime(BString::from("the Composer instance of this event is gone"))
            })?;
            let rhandle = register_composer_entity(&composer);
            Ok(rust_handle_value(rhandle, BStr::new("Composer\\Composer")))
        }
        b"getIO" => {
            let io = match (
                script_event(event),
                package_event(event),
                installer_event(event),
            ) {
                (Some(event), _, _) => event.get_io(),
                (_, Some(event), _) => event.get_io(),
                (_, _, Some(event)) => event.get_io(),
                _ => {
                    return Err(PhpThrow::runtime(BString::from(
                        "getIO is only available on script, package and installer events",
                    )));
                }
            };
            let class =
                io_stub_class(&io).map_err(|error| PhpThrow::runtime(error_message(&error)))?;
            let rhandle = register_io_entity(&io);
            Ok(rust_handle_value(rhandle, class))
        }
        b"getLocalRepo" => match package_event(event) {
            Some(event) => repository_handle_value(&event.get_local_repo().as_repository_handle()),
            None => Err(PhpThrow::runtime(BString::from(
                "getLocalRepo is only available on package events",
            ))),
        },
        b"getOperations" => match package_event(event) {
            Some(event) => Ok(PluginValue::List(
                event
                    .get_operations()
                    .iter()
                    .map(operation_handle_value)
                    .collect(),
            )),
            None => Err(PhpThrow::runtime(BString::from(
                "getOperations is only available on package events",
            ))),
        },
        b"getOperation" => match package_event(event) {
            Some(event) => Ok(operation_handle_value(event.get_operation())),
            None => Err(PhpThrow::runtime(BString::from(
                "getOperation is only available on package events",
            ))),
        },
        b"getCommandName" => match event.as_any().downcast_ref::<CommandEvent>() {
            Some(event) => Ok(PluginValue::string(event.get_command_name())),
            None => Err(PhpThrow::runtime(BString::from(
                "getCommandName is only available on command events",
            ))),
        },
        b"getCommand" => match event.as_any().downcast_ref::<PreCommandRunEvent>() {
            Some(event) => Ok(PluginValue::string(event.get_command())),
            None => Err(PhpThrow::runtime(BString::from(
                "getCommand is only available on pre-command-run events",
            ))),
        },
        // TODO(plugin): symfony/console lives in both worlds independently, and nothing hands
        // the Rust-side console input and output to the plugin process yet.
        b"getInput" | b"getOutput"
            if event.as_any().downcast_ref::<CommandEvent>().is_some()
                || event
                    .as_any()
                    .downcast_ref::<PreCommandRunEvent>()
                    .is_some() =>
        {
            Err(PhpThrow::runtime(bformat!(
                "the console {} of a command event is not available over RPC yet",
                match method_name.as_bytes() {
                    b"getInput" => "input",
                    b"getOutput" => "output",
                    _ => unreachable!(),
                }
            )))
        }
        b"isExecutingOperations" => match installer_event(event) {
            Some(event) => Ok(PluginValue::Bool(event.is_executing_operations())),
            None => Err(PhpThrow::runtime(BString::from(
                "isExecutingOperations is only available on installer events",
            ))),
        },
        b"getTransaction" => match installer_event(event) {
            Some(event) => match event.get_transaction() {
                Some(transaction) => Ok(transaction_handle_value(transaction)),
                None => Ok(PluginValue::Null),
            },
            None => Err(PhpThrow::runtime(BString::from(
                "getTransaction is only available on installer events",
            ))),
        },
        other => {
            if let Some(event) = event.as_any().downcast_ref::<PostFileDownloadEvent>() {
                return dispatch_post_file_download_event_method(event, BStr::new(other));
            }
            if let Some(event) = event.as_any_mut().downcast_mut::<PrePoolCreateEvent>() {
                return dispatch_pre_pool_create_event_method(event, BStr::new(other), args);
            }
            match event.as_any_mut().downcast_mut::<PreFileDownloadEvent>() {
                Some(event) => {
                    dispatch_pre_file_download_event_method(event, BStr::new(other), args)
                }
                // TODO(plugin): the rest need full proxying of the object graph an event exposes,
                // which does not exist yet.
                None => Err(unavailable_method("Event", other)),
            }
        }
    }
}

fn script_event(event: &dyn EventInterface) -> Option<&ScriptEvent> {
    event.as_any().downcast_ref::<ScriptEvent>()
}

fn package_event(event: &dyn EventInterface) -> Option<&PackageEvent> {
    event.as_any().downcast_ref::<PackageEvent>()
}

fn installer_event(event: &dyn EventInterface) -> Option<&InstallerEvent> {
    event.as_any().downcast_ref::<InstallerEvent>()
}
