//! The methods a `Composer\Downloader\DownloadManager` entity answers over RPC.

use crate::composer::ComposerHandle;
use crate::downloader::DownloadManagerInterface;
use crate::exception::IrrecoverableDownloadException;
use crate::package::PackageInterfaceHandle;
use crate::plugin::job_registry;
use crate::plugin::job_registry::JobOwner;
use crate::plugin::rpc::codec::arg;
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
use crate::plugin::rpc::table::{R_TABLE, RustEntity};
use crate::util::HttpDownloader;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{PhpMixed, bformat, path_from_bytes};

/// The download manager's contract is asynchronous on both sides: PHP declares a
/// `PromiseInterface` return, Rust an `async fn`. Each call registers its future as a job and
/// answers with the job's pending promise.
pub(crate) fn dispatch_download_manager_method(
    dm: &std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let string_arg = |position: usize| arg::<BString>(method_name, args, position);
    let path_arg = |position: usize| {
        arg::<bstr::BString>(method_name, args, position)
            .map(|path| std::path::PathBuf::from(path_from_bytes(&path)))
    };
    let package_arg = |position: usize| arg::<PackageInterfaceHandle>(method_name, args, position);
    let optional_package_arg =
        |position: usize| arg::<Option<PackageInterfaceHandle>>(method_name, args, position);
    let future: std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<Option<PhpMixed>>>>,
    > = match method_name.as_bytes() {
        b"setPreferSource" => {
            let preferred = arg::<bool>(method_name, args, 0)?;
            dm.borrow().set_prefer_source(preferred);
            return Ok(PluginValue::Null);
        }
        b"setPreferDist" => {
            let preferred = arg::<bool>(method_name, args, 0)?;
            dm.borrow().set_prefer_dist(preferred);
            return Ok(PluginValue::Null);
        }
        b"download" => {
            let (dm, package, path, prev) = (
                dm.clone(),
                package_arg(0)?,
                path_arg(1)?,
                optional_package_arg(2)?,
            );
            Box::pin(async move { dm.borrow().download(package, &path, prev).await })
        }
        b"prepare" => {
            let (dm, r#type, package, path, prev) = (
                dm.clone(),
                string_arg(0)?,
                package_arg(1)?,
                path_arg(2)?,
                optional_package_arg(3)?,
            );
            Box::pin(async move {
                dm.borrow()
                    .prepare(BStr::new(&r#type), package, &path, prev)
                    .await
            })
        }
        b"install" => {
            let (dm, package, path) = (dm.clone(), package_arg(0)?, path_arg(1)?);
            Box::pin(async move { dm.borrow().install(package, &path).await })
        }
        b"update" => {
            let (dm, initial, target, path) =
                (dm.clone(), package_arg(0)?, package_arg(1)?, path_arg(2)?);
            Box::pin(async move { dm.borrow().update(initial, target, &path).await })
        }
        b"remove" => {
            let (dm, package, path) = (dm.clone(), package_arg(0)?, path_arg(1)?);
            Box::pin(async move { dm.borrow().remove(package, &path).await })
        }
        b"cleanup" => {
            let (dm, r#type, package, path, prev) = (
                dm.clone(),
                string_arg(0)?,
                package_arg(1)?,
                path_arg(2)?,
                optional_package_arg(3)?,
            );
            Box::pin(async move {
                dm.borrow()
                    .cleanup(BStr::new(&r#type), package, &path, prev)
                    .await
            })
        }
        // TODO(plugin): the downloader-facing surface (getDownloader / setDownloader /
        // getDownloaderForPackage / getDownloaderType) needs proxy stubs for
        // DownloaderInterface implementations before it can cross.
        other => {
            return Err(unavailable_method("DownloadManager", other));
        }
    };

    // Under Composer the promise is a chain over the job the downloader started on the graph's
    // HttpDownloader, so that is the service whose `countActiveJobs()` keeps a wait going until
    // this job is done.
    let downloader = graph_http_downloader(dm)?;
    let started = std::rc::Rc::new(std::cell::Cell::new(false));
    let method = method_name.to_owned();
    Ok(job_registry::register(
        JobOwner::http_downloader(&downloader),
        downloader,
        started.clone(),
        // TODO(async): Composer's cancel reaches the request or the process at the root of the
        // chain and rejects with that job's own exception; the root is not known here.
        error_throw(
            BStr::new(""),
            &IrrecoverableDownloadException::new(bformat!(
                "DownloadManager::{method_name}() canceled"
            ))
            .into(),
        ),
        Box::pin(async move {
            started.set(true);
            future
                .await
                .map(|resolved| resolved.to_wire())
                .map_err(|error| error_throw(bformat!("{method} failed over RPC"), &error))
        }),
    ))
}

/// The HttpDownloader of the object graph `dm` belongs to: the one the Loop of its Composer
/// instance drives.
fn graph_http_downloader(
    dm: &std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
) -> Result<std::rc::Rc<std::cell::RefCell<HttpDownloader>>, PhpThrow> {
    let composers: Vec<ComposerHandle> = R_TABLE.with(|table| {
        table
            .borrow()
            .values()
            .filter_map(|entity| match entity {
                RustEntity::Composer(composer) => Some(composer.clone()),
                _ => None,
            })
            .collect()
    });
    for composer in composers {
        let composer = composer.borrow();
        if std::rc::Rc::ptr_eq(&composer.get_download_manager(), dm) {
            return Ok(composer.get_loop().borrow().get_http_downloader().clone());
        }
    }
    Err(PhpThrow::runtime(
        "this DownloadManager belongs to no Composer instance the plugin process holds",
    ))
}
