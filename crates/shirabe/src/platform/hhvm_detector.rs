//! ref: composer/src/Composer/Platform/HhvmDetector.php

use crate::util::Platform;
use crate::util::ProcessExecutor;
use shirabe_external_packages::symfony::process::ExecutableFinder;
use shirabe_php_shim::{HHVM_VERSION, defined};
use std::sync::Mutex;

// None = null (uninitialized), Some(None) = false (not found), Some(Some(v)) = version
static HHVM_VERSION_CACHE: Mutex<Option<Option<String>>> = Mutex::new(None);

#[derive(Debug)]
pub struct HhvmDetector {
    executable_finder: Option<ExecutableFinder>,
    process_executor: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
}

impl HhvmDetector {
    pub fn new(
        executable_finder: Option<ExecutableFinder>,
        process_executor: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
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
            let finder = self
                .executable_finder
                .get_or_insert_with(ExecutableFinder::new);
            let hhvm_path = finder.find("hhvm", None, &[]);
            if let Some(hhvm_path) = hhvm_path {
                let executor = self.process_executor.get_or_insert_with(|| {
                    std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(None)))
                });
                let mut version_output = shirabe_php_shim::PhpMixed::Null;
                let cmd = shirabe_php_shim::PhpMixed::List(
                    [
                        hhvm_path.as_str(),
                        "--php",
                        "-d",
                        "hhvm.jit=0",
                        "-r",
                        "echo HHVM_VERSION;",
                    ]
                    .into_iter()
                    .map(|s| shirabe_php_shim::PhpMixed::String(s.to_string()))
                    .collect(),
                );
                let exit_code = executor
                    .borrow_mut()
                    .execute(cmd, Some(&mut version_output), ())
                    .unwrap_or(1);
                if exit_code == 0 {
                    *cache = Some(version_output.as_string().map(|s| s.to_string()));
                }
            }
        }

        cache.clone().flatten()
    }
}
