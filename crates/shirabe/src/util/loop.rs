//! ref: composer/src/Composer/Util/Loop.php sha1:54f0907047c3f5d67ef41b69d9696ed6a88804e2

use crate::plugin::job_registry;
use crate::plugin::job_registry::JobOwner;
use crate::util::{HttpDownloader, ProcessExecutor};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use shirabe_php_shim::microtime;
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
        mut progress: Option<&mut ProgressBar>,
    ) -> anyhow::Result<()> {
        let mut pending: FuturesUnordered<_> = promises.into_iter().collect();
        let mut uncaught: Option<anyhow::Error> = None;
        let owners = self.job_owners();
        // The awaits a wait further up the stack is parked on are not this group's.
        let parked_before = job_registry::parked_php_jobs();
        let mut progress_started = false;
        let mut last_update = 0.0;
        let mut progress_error: Option<anyhow::Error> = None;

        std::future::poll_fn(|cx| {
            // PHP's jobs start inside `add()` / `executeAsync()`, before the wait; these futures
            // start on their first poll, so the bar is sized after the first pass over them.
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

            let active_jobs = self.count_active_jobs();
            if let Some(progress) = progress.as_deref_mut() {
                let mut update = || -> anyhow::Result<()> {
                    if !progress_started {
                        progress_started = true;
                        progress.start(Some(active_jobs))?;
                    }
                    if microtime() - last_update > 0.1 {
                        last_update = microtime();
                        progress.set_progress(progress.get_max_steps() - active_jobs)?;
                    }
                    Ok(())
                };
                if let Err(e) = update() {
                    progress_error = Some(e);
                    return std::task::Poll::Ready(());
                }
            }

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

        if let Some(e) = progress_error {
            return Err(e);
        }

        // as we skip progress updates if they are too quick, make sure we do one last one here at 100%
        if let Some(progress) = progress {
            progress.finish()?;
        }

        uncaught.map_or(Ok(()), Err)
    }

    fn count_active_jobs(&self) -> i64 {
        self.job_owners()
            .into_iter()
            .map(job_registry::count_active_jobs)
            .sum()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::io::{IOInterface, NullIO};
    use indexmap::IndexMap;
    use shirabe_symfony_console::output::BufferedOutput;

    #[test]
    fn wait_advances_progress_over_active_jobs() {
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
        let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
        let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
            HttpDownloader::new(io, config, IndexMap::new(), true).unwrap(),
        ));
        let process_executor =
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)));
        let r#loop = Loop::new(http_downloader, Some(process_executor.clone()));

        let output = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
            None, false, None,
        )));
        let mut progress = ProgressBar::new(output.clone(), 0, 0.0);

        let promises: Vec<
            std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>>,
        > = (0..2)
            .map(|_| {
                let process = process_executor
                    .borrow()
                    .execute_async(&["sleep", "0.2"], None);
                Box::pin(async move { process.await.map(|_| ()) })
                    as std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>>
            })
            .collect();

        crate::util::sync_executor::block_on(r#loop.wait(promises, Some(&mut progress))).unwrap();

        assert_eq!(2, progress.get_max_steps());
        let rendered = output.borrow().fetch();
        assert!(rendered.starts_with(b" 0/2 ["), "{rendered:?}");
        assert!(
            rendered.ends_with(b" 2/2 [============================] 100%"),
            "{rendered:?}"
        );
    }
}
