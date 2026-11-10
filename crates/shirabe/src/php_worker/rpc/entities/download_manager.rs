//! The methods a `Composer\Downloader\DownloadManager` entity answers over RPC.

use crate::composer::ComposerHandle;
use crate::downloader::DownloadManagerInterface;
use crate::exception::IrrecoverableDownloadException;
use crate::package::PackageInterfaceHandle;
use crate::php_worker::job;
use crate::php_worker::job::JobOwner;
use crate::php_worker::rpc::dispatch::error_throw;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::{forwarding, table};
use crate::util::HttpDownloader;
use bstr::{BStr, BString};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToPluginValue};
use shirabe_php_shim::{PhpMixed, bformat, path_from_bytes};

impl Entity for std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>> {
    const EXPECTED: &'static [u8] = b"a DownloadManager";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(&forwarding::downloader::download_manager::CROSSING)
    }
}

fn path(call: &Call<'_>, position: usize) -> Result<std::path::PathBuf, PhpThrow> {
    call.arg::<BString>(position)
        .map(|path| std::path::PathBuf::from(path_from_bytes(&path)))
}

/// The download manager's contract is asynchronous on both sides: PHP declares a
/// `PromiseInterface` return, Rust an `async fn`. Each call registers its future as a job and
/// answers with the job's pending promise.
fn promise(
    dm: &std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
    call: &Call<'_>,
    future: std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Option<PhpMixed>>>>>,
) -> Result<PluginValue, PhpThrow> {
    // Under Composer the promise is a chain over the job the downloader started on the graph's
    // HttpDownloader, so that is the service whose `countActiveJobs()` keeps a wait going until
    // this job is done.
    let downloader = graph_http_downloader(dm)?;
    let started = std::rc::Rc::new(std::cell::Cell::new(false));
    let failed = call.failed();
    Ok(job::register(
        JobOwner::http_downloader(&downloader),
        downloader,
        started.clone(),
        // TODO(async): Composer's cancel reaches the request or the process at the root of the
        // chain and rejects with that job's own exception; the root is not known here.
        error_throw(
            BStr::new(""),
            &IrrecoverableDownloadException::new(bformat!(
                "DownloadManager::{}() canceled",
                call.method()
            ))
            .into(),
        ),
        Box::pin(async move {
            started.set(true);
            future
                .await
                .map(|resolved| resolved.to_plugin_value())
                .map_err(failed)
        }),
    ))
}

// TODO(plugin): the downloader-facing surface (getDownloader / setDownloader /
// getDownloaderForPackage / getDownloaderType) needs proxy stubs for DownloaderInterface
// implementations before it can cross.
impl forwarding::downloader::download_manager::Forwarding
    for forwarding::downloader::download_manager::PhpClass
{
    type Handle = std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>;

    fn set_prefer_source(dm: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        dm.borrow().set_prefer_source(call.arg::<bool>(0)?);
        Ok(PluginValue::Null)
    }

    fn set_prefer_dist(dm: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        dm.borrow().set_prefer_dist(call.arg::<bool>(0)?);
        Ok(PluginValue::Null)
    }

    fn download(dm: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let (manager, package, path, prev) = (
            dm.clone(),
            call.arg::<PackageInterfaceHandle>(0)?,
            path(call, 1)?,
            call.arg::<Option<PackageInterfaceHandle>>(2)?,
        );
        promise(
            dm,
            call,
            Box::pin(async move { manager.borrow().download(package, &path, prev).await }),
        )
    }

    fn prepare(dm: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let (manager, r#type, package, path, prev) = (
            dm.clone(),
            call.arg::<BString>(0)?,
            call.arg::<PackageInterfaceHandle>(1)?,
            path(call, 2)?,
            call.arg::<Option<PackageInterfaceHandle>>(3)?,
        );
        promise(
            dm,
            call,
            Box::pin(async move {
                manager
                    .borrow()
                    .prepare(BStr::new(&r#type), package, &path, prev)
                    .await
            }),
        )
    }

    fn install(dm: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let (manager, package, path) = (
            dm.clone(),
            call.arg::<PackageInterfaceHandle>(0)?,
            path(call, 1)?,
        );
        promise(
            dm,
            call,
            Box::pin(async move { manager.borrow().install(package, &path).await }),
        )
    }

    fn update(dm: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let (manager, initial, target, path) = (
            dm.clone(),
            call.arg::<PackageInterfaceHandle>(0)?,
            call.arg::<PackageInterfaceHandle>(1)?,
            path(call, 2)?,
        );
        promise(
            dm,
            call,
            Box::pin(async move { manager.borrow().update(initial, target, &path).await }),
        )
    }

    fn remove(dm: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let (manager, package, path) = (
            dm.clone(),
            call.arg::<PackageInterfaceHandle>(0)?,
            path(call, 1)?,
        );
        promise(
            dm,
            call,
            Box::pin(async move { manager.borrow().remove(package, &path).await }),
        )
    }

    fn cleanup(dm: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        let (manager, r#type, package, path, prev) = (
            dm.clone(),
            call.arg::<BString>(0)?,
            call.arg::<PackageInterfaceHandle>(1)?,
            path(call, 2)?,
            call.arg::<Option<PackageInterfaceHandle>>(3)?,
        );
        promise(
            dm,
            call,
            Box::pin(async move {
                manager
                    .borrow()
                    .cleanup(BStr::new(&r#type), package, &path, prev)
                    .await
            }),
        )
    }
}

/// The HttpDownloader of the object graph `dm` belongs to: the one the Loop of its Composer
/// instance drives.
fn graph_http_downloader(
    dm: &std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
) -> Result<std::rc::Rc<std::cell::RefCell<HttpDownloader>>, PhpThrow> {
    for composer in table::entities::<ComposerHandle>() {
        let composer = composer.borrow();
        if std::rc::Rc::ptr_eq(&composer.get_download_manager(), dm) {
            return Ok(composer.get_loop().borrow().get_http_downloader().clone());
        }
    }
    Err(PhpThrow::runtime(
        "this DownloadManager belongs to no Composer instance the plugin process holds",
    ))
}
