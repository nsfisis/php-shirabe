//! ref: composer/src/Composer/Util/Loop.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::microtime;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::symfony::component::console::helper::progress_bar::ProgressBar;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct Loop {
    http_downloader: HttpDownloader,
    process_executor: Option<ProcessExecutor>,
    current_promises: IndexMap<i64, Vec<Box<dyn PromiseInterface>>>,
    wait_index: i64,
}

impl Loop {
    pub fn new(mut http_downloader: HttpDownloader, process_executor: Option<ProcessExecutor>) -> Self {
        http_downloader.enable_async();

        let process_executor = process_executor.map(|mut pe| {
            pe.enable_async();
            pe
        });

        Self {
            http_downloader,
            process_executor,
            current_promises: IndexMap::new(),
            wait_index: 0,
        }
    }

    pub fn get_http_downloader(&self) -> &HttpDownloader {
        &self.http_downloader
    }

    pub fn get_process_executor(&self) -> Option<&ProcessExecutor> {
        self.process_executor.as_ref()
    }

    pub fn wait(&mut self, promises: Vec<Box<dyn PromiseInterface>>, progress: Option<&mut ProgressBar>) -> Result<()> {
        let mut uncaught: Option<anyhow::Error> = None;

        shirabe_external_packages::react::promise::all(&promises).then(
            || {},
            |e: anyhow::Error| {
                uncaught = Some(e);
            },
        );

        // keep track of every group of promises that is waited on, so abortJobs can
        // cancel them all, even if wait() was called within a wait()
        let wait_index = self.wait_index;
        self.wait_index += 1;
        self.current_promises.insert(wait_index, promises);

        if let Some(ref progress) = progress {
            let mut total_jobs: i64 = 0;
            total_jobs += self.http_downloader.count_active_jobs();
            if let Some(ref pe) = self.process_executor {
                total_jobs += pe.count_active_jobs();
            }
            progress.start(total_jobs);
        }

        let mut last_update: f64 = 0.0;
        loop {
            let mut active_jobs: i64 = 0;

            active_jobs += self.http_downloader.count_active_jobs();
            if let Some(ref pe) = self.process_executor {
                active_jobs += pe.count_active_jobs();
            }

            if let Some(ref progress) = progress {
                if microtime(true) - last_update > 0.1 {
                    last_update = microtime(true);
                    progress.set_progress(progress.get_max_steps() - active_jobs);
                }
            }

            if active_jobs == 0 {
                break;
            }
        }

        // as we skip progress updates if they are too quick, make sure we do one last one here at 100%
        if let Some(ref progress) = progress {
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
            for promise in promise_group {
                // to support react/promise 2.x we wrap the promise in a resolve() call for safety
                shirabe_external_packages::react::promise::resolve(Some(promise)).cancel();
            }
        }
    }
}
