//! ref: composer/src/Composer/Util/Loop.php

use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use shirabe_external_packages::symfony::console::helper::ProgressBar;

#[derive(Debug)]
pub struct Loop {
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    process_executor: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
}

impl Loop {
    pub fn new(
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        process_executor: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
        http_downloader.borrow_mut().enable_async();

        let process_executor = process_executor.inspect(|pe| {
            pe.borrow_mut().enable_async();
        });

        Self {
            http_downloader,
            process_executor,
        }
    }

    pub fn get_http_downloader(&self) -> &std::rc::Rc<std::cell::RefCell<HttpDownloader>> {
        &self.http_downloader
    }

    pub fn get_process_executor(
        &self,
    ) -> Option<&std::rc::Rc<std::cell::RefCell<ProcessExecutor>>> {
        self.process_executor.as_ref()
    }

    pub async fn wait<'p>(
        &mut self,
        promises: Vec<
            std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + 'p>>,
        >,
        _progress: Option<&mut ProgressBar>,
    ) -> anyhow::Result<()> {
        let mut pending: FuturesUnordered<_> = promises.into_iter().collect();
        let mut uncaught: Option<anyhow::Error> = None;

        // TODO(phase-c-promise): promises are now polled concurrently via FuturesUnordered, but
        // each individual future (HttpDownloader::add/add_copy etc.) still resolves through a
        // blocking bridge (curl_runtime()/sync_executor::block_on), so real I/O overlap does not
        // happen yet — the bridged future fully blocks the thread until it settles before the next
        // one gets polled. That only changes once a single top-level Runtime replaces those bridges.
        // The PHP progress bar is tied to the worker active-job count and is also deferred until then.
        while let Some(result) = pending.next().await {
            if let Err(e) = result
                && uncaught.is_none()
            {
                uncaught = Some(e);
            }
        }

        uncaught.map_or(Ok(()), Err)
    }

    pub fn abort_jobs(&self) {
        // TODO(phase-c-promise): no-op until a cancellation mechanism is introduced. PHP cancels
        // every in-flight promise group it tracks in $currentPromises; reintroduce that tracking
        // once the asynchronous workers support cancellation on a multi-thread runtime.
    }
}
