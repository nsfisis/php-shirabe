//! ref: composer/src/Composer/Util/Loop.php

use crate::plugin::job_registry;
use crate::plugin::job_registry::JobOwner;
use crate::util::{HttpDownloader, ProcessExecutor};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use shirabe_symfony_console::helper::ProgressBar;

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
        &self,
        promises: Vec<
            std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + 'p>>,
        >,
        _progress: Option<&mut ProgressBar>,
    ) -> anyhow::Result<()> {
        let mut pending: FuturesUnordered<_> = promises.into_iter().collect();
        let mut uncaught: Option<anyhow::Error> = None;
        let owners = self.job_owners();
        // The awaits a wait further up the stack is parked on are not this group's.
        let parked_before = job_registry::parked_php_jobs();

        // TODO(async): `_progress` is not advanced here. PHP sets the bar from the two services'
        // `countActiveJobs()` on every pass of its loop; `InstallationManager::wait_on_promises`
        // renders a single 0% -> 100% step after this returns instead.
        std::future::poll_fn(|cx| {
            while let std::task::Poll::Ready(Some(result)) = pending.poll_next_unpin(cx) {
                if let Err(e) = result
                    && uncaught.is_none()
                {
                    uncaught = Some(e);
                }
            }

            // PHP's wait ends when the two services report no active job, which covers the jobs
            // plugin code started on them and never waited on. Those sit in the job registry; it
            // is empty in a run that never started the plugin worker.
            let jobs_left = match job_registry::poll_jobs_in_wait(&owners, cx) {
                Ok(jobs_left) => jobs_left,
                Err(e) => {
                    if uncaught.is_none() {
                        uncaught = Some(e);
                    }
                    false
                }
            };

            if pending.is_empty() && !jobs_left {
                return std::task::Poll::Ready(());
            }

            // A promise plugin code built settles only while PHP code runs. With no job left to
            // push a settle and every remaining future parked on such a promise, nothing can run
            // that code any more; PHP spins in its wait loop forever at this point.
            if !jobs_left && !pending.is_empty() {
                let parked: Vec<u64> = job_registry::parked_php_jobs()
                    .into_iter()
                    .filter(|id| !parked_before.contains(id))
                    .collect();
                if parked.len() >= pending.len() {
                    job_registry::fail_php_jobs(&parked);
                }
            }

            std::task::Poll::Pending
        })
        .await;

        uncaught.map_or(Ok(()), Err)
    }

    fn job_owners(&self) -> Vec<JobOwner> {
        let mut owners = vec![JobOwner::http_downloader(&self.http_downloader)];
        if let Some(process_executor) = &self.process_executor {
            owners.push(JobOwner::process_executor(process_executor));
        }
        owners
    }

    pub fn abort_jobs(&self) {
        // TODO(async): only the requests the Rust side awaits on the downloader are cancelled.
        // PHP cancels every promise of the groups being waited on, which also reaches the
        // processes they started.
        job_registry::abort_internal_jobs(Some(&self.job_owners()));
    }
}
