//! ref: composer/src/Composer/Package/Loader/InvalidPackageException.php

use shirabe_php_shim::{Exception, PhpMixed};

#[derive(Debug)]
pub struct InvalidPackageException {
    inner: Exception,
    errors: Vec<String>,
    warnings: Vec<String>,
    data: Vec<PhpMixed>,
}

impl InvalidPackageException {
    pub fn new(errors: Vec<String>, warnings: Vec<String>, data: Vec<PhpMixed>) -> Self {
        let message = format!(
            "Invalid package information: \n{}",
            errors
                .iter()
                .chain(warnings.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        Self {
            inner: Exception { message, code: 0 },
            errors,
            warnings,
            data,
        }
    }

    pub fn get_data(&self) -> &[PhpMixed] {
        &self.data
    }

    pub fn get_errors(&self) -> &[String] {
        &self.errors
    }

    pub fn get_warnings(&self) -> &[String] {
        &self.warnings
    }
}
