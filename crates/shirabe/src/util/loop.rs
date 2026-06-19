//! ref: composer/src/Composer/Util/Loop.php

use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;
use anyhow::Result;
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
        promises: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'p>>>,
        _progress: Option<&mut ProgressBar>,
    ) -> Result<()> {
        let mut uncaught: Option<anyhow::Error> = None;

        // TODO(phase-c-promise): the asynchronous worker classes (HttpDownloader / ProcessExecutor)
        // run single-threaded for now, so the promises are consumed serially. Once the workers run
        // on a multi-thread runtime these futures should be driven concurrently instead of in order.
        // The PHP progress bar is tied to the worker active-job count and is also deferred until then.
        for promise in promises {
            if let Err(e) = promise.await
                && uncaught.is_none()
            {
                uncaught = Some(e);
            }
        }

        if let Some(e) = uncaught {
            return Err(e);
        }

        Ok(())
    }

    pub fn abort_jobs(&self) {
        // TODO(phase-c-promise): no-op until a cancellation mechanism is introduced. PHP cancels
        // every in-flight promise group it tracks in $currentPromises; reintroduce that tracking
        // once the asynchronous workers support cancellation on a multi-thread runtime.
    }
}
