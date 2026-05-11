//! ref: composer/src/Composer/Platform/HhvmDetector.php

use std::sync::Mutex;
use shirabe_external_packages::symfony::process::executable_finder::ExecutableFinder;
use shirabe_php_shim::{defined, HHVM_VERSION};
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

// None = null (uninitialized), Some(None) = false (not found), Some(Some(v)) = version
static HHVM_VERSION_CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

pub struct HhvmDetector {
    executable_finder: Option<ExecutableFinder>,
    process_executor: Option<ProcessExecutor>,
}

impl HhvmDetector {
    pub fn new(executable_finder: Option<ExecutableFinder>, process_executor: Option<ProcessExecutor>) -> Self {
        Self {
            executable_finder,
            process_executor,
        }
    }

    pub fn reset(&self) {
        *HHVM_VERSION_CACHE.lock().unwrap() = None;
    }

    pub fn get_version(&mut self) -> Option<String> {
        let cached = HHVM_VERSION_CACHE.lock().unwrap().clone();
        if cached.is_some() {
            return cached.flatten();
        }

        let mut cache = HHVM_VERSION_CACHE.lock().unwrap();
        *cache = Some(if defined("HHVM_VERSION") {
            HHVM_VERSION.map(|s| s.to_string())
        } else {
            None
        });

        if cache.as_ref().unwrap().is_none() && !Platform::is_windows() {
            *cache = Some(None);
            let finder = self.executable_finder.get_or_insert_with(ExecutableFinder::new);
            let hhvm_path = finder.find("hhvm");
            if let Some(hhvm_path) = hhvm_path {
                let executor = self.process_executor.get_or_insert_with(ProcessExecutor::new);
                let mut version_output = String::new();
                let exit_code = executor.execute(
                    &[&hhvm_path, "--php", "-d", "hhvm.jit=0", "-r", "echo HHVM_VERSION;"],
                    &mut version_output,
                );
                if exit_code == 0 {
                    *cache = Some(Some(version_output));
                }
            }
        }

        cache.clone().flatten()
    }
}
