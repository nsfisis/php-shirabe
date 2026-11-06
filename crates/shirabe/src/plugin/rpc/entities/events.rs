//! The methods the plugin events (`PreFileDownloadEvent`, `PostFileDownloadEvent`,
//! `PrePoolCreateEvent`) declare beyond the base event's, answered over RPC.

use crate::package::PackageInterfaceHandle;
use crate::plugin::post_file_download_event::PostFileDownloadEvent;
use crate::plugin::pre_file_download_event::{FileDownloadContext, PreFileDownloadEvent};
use crate::plugin::pre_pool_create_event::PrePoolCreateEvent;
use crate::plugin::rpc::codec::arg;
use crate::plugin::rpc::dispatch::unavailable_method;
use crate::plugin::rpc::table::{
    RustEntity, register_entity, repository_handle_value, rust_handle_value,
};
use crate::util::StreamContextOptions;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};

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
