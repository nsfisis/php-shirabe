//! The methods a `Composer\Downloader\DownloadManager` entity answers over RPC.

use crate::composer::ComposerHandle;
use crate::downloader::{__php_binding_download_manager_interface, DownloadManagerInterface};
use crate::exception::IrrecoverableDownloadException;
use crate::php_worker::job::JobOwner;
use crate::php_worker::rpc::binding::JobContext;
use crate::php_worker::rpc::dispatch::error_throw;
use crate::php_worker::rpc::entity::Call;
use crate::php_worker::rpc::table;
use crate::util::HttpDownloader;
use bstr::BStr;
use shirabe_php_binding_macros::php_entity;
use shirabe_php_rpc::PhpThrow;
use shirabe_php_shim::bformat;

// The download manager's contract is asynchronous on both sides: PHP declares a
// `PromiseInterface` return, Rust an `async fn`. Each call registers its future as a job and
// answers with the job's pending promise.
//
// TODO(plugin): the downloader-facing surface (getDownloader / setDownloader /
// getDownloaderForPackage / getDownloaderType) needs proxy stubs for DownloaderInterface
// implementations before it can cross.
php_entity! {
    handle: std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
    expected: "a DownloadManager",
    class: r"Composer\Downloader\DownloadManager",
    types: {
        r"Composer\Downloader\DownloadManager": instance __php_binding_download_manager_interface
            for dyn DownloadManagerInterface {
            via: |dm| dm.borrow(),
            job: job,
        },
    },
}

/// The job a call starts. Under Composer the promise is a chain over the job the downloader
/// started on the graph's HttpDownloader, so that is the service whose `countActiveJobs()` keeps a
/// wait going until this job is done.
fn job(
    dm: &std::rc::Rc<std::cell::RefCell<dyn DownloadManagerInterface>>,
    call: &Call<'_>,
) -> Result<JobContext, PhpThrow> {
    let downloader = graph_http_downloader(dm)?;
    Ok(JobContext {
        owner: JobOwner::http_downloader(&downloader),
        owner_entity: downloader,
        // TODO(async): Composer's cancel reaches the request or the process at the root of the
        // chain and rejects with that job's own exception; the root is not known here.
        canceled: error_throw(
            BStr::new(""),
            &IrrecoverableDownloadException::new(bformat!(
                "DownloadManager::{}() canceled",
                call.method()
            ))
            .into(),
        ),
    })
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
