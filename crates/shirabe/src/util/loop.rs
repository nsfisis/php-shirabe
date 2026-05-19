//! ref: composer/src/Composer/Util/Loop.php

use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::symfony::component::console::helper::progress_bar::ProgressBar;
use shirabe_php_shim::microtime;

pub struct Loop {
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    process_executor: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    current_promises: IndexMap<i64, Vec<Box<dyn PromiseInterface>>>,
    wait_index: i64,
}

impl std::fmt::Debug for Loop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loop")
            .field("http_downloader", &self.http_downloader)
            .field("process_executor", &self.process_executor)
            .field("wait_index", &self.wait_index)
            .finish()
    }
}

impl Loop {
    pub fn new(
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        process_executor: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
        http_downloader.borrow_mut().enable_async();

        let process_executor = process_executor.map(|pe| {
            pe.borrow_mut().enable_async();
            pe
        });

        Self {
            http_downloader,
            process_executor,
            current_promises: IndexMap::new(),
            wait_index: 0,
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

    pub fn wait(
        &mut self,
        promises: Vec<Box<dyn PromiseInterface>>,
        mut progress: Option<&mut ProgressBar>,
    ) -> Result<()> {
        let uncaught: Option<anyhow::Error> = None;

        // TODO(phase-b): Promise::then captures uncaught by Fn; needs a Cell/RefCell wrapper
        // and a thunk that matches FnOnce(Option<PhpMixed>) -> Option<PhpMixed>.
        let _ = shirabe_external_packages::react::promise::all(
            promises
                .iter()
                .map(|_| todo!("clone Box<dyn PromiseInterface>"))
                .collect(),
        );

        // keep track of every group of promises that is waited on, so abortJobs can
        // cancel them all, even if wait() was called within a wait()
        let wait_index = self.wait_index;
        self.wait_index += 1;
        self.current_promises.insert(wait_index, promises);

        if let Some(ref mut progress) = progress {
            let mut total_jobs: i64 = 0;
            total_jobs += self.http_downloader.borrow_mut().count_active_jobs(None);
            if let Some(ref pe) = self.process_executor {
                total_jobs += pe.borrow_mut().count_active_jobs(None);
            }
            progress.start(Some(total_jobs));
        }

        let mut last_update: f64 = 0.0;
        loop {
            let mut active_jobs: i64 = 0;

            active_jobs += self.http_downloader.borrow_mut().count_active_jobs(None);
            if let Some(ref pe) = self.process_executor {
                active_jobs += pe.borrow_mut().count_active_jobs(None);
            }

            if let Some(ref mut progress) = progress {
                if microtime(true) - last_update > 0.1 {
                    last_update = microtime(true);
                    let new_progress = progress.get_max_steps() - active_jobs;
                    progress.set_progress(new_progress);
                }
            }

            if active_jobs == 0 {
                break;
            }
        }

        // as we skip progress updates if they are too quick, make sure we do one last one here at 100%
        if let Some(ref mut progress) = progress {
            progress.finish();
        }

        self.current_promises.remove(&wait_index);
        if let Some(e) = uncaught {
            return Err(e);
        }

        Ok(())
    }

    pub fn abort_jobs(&self) {
        for promise_group in self.current_promises.values() {
            for _promise in promise_group {
                // TODO(phase-b): cancel requires CancellablePromiseInterface; PromiseInterface trait
                // doesn't expose it. Drop the wrap+cancel until we have the right trait.
            }
        }
    }
}
