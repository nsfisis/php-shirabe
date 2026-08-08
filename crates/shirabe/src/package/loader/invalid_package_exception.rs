//! ref: composer/src/Composer/Package/Loader/InvalidPackageException.php

use indexmap::IndexMap;
use shirabe_php_shim::{Exception, PhpMixed};

#[derive(Debug)]
pub struct InvalidPackageException {
    inner: Exception,
    errors: Vec<String>,
    warnings: Vec<String>,
    data: IndexMap<String, PhpMixed>,
}

impl InvalidPackageException {
    pub fn new(
        errors: Vec<String>,
        warnings: Vec<String>,
        data: IndexMap<String, PhpMixed>,
    ) -> Self {
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
            inner: Exception::new(message),
            errors,
            warnings,
            data,
        }
    }

    pub fn get_data(&self) -> &IndexMap<String, PhpMixed> {
        &self.data
    }

    pub fn get_errors(&self) -> &[String] {
        &self.errors
    }

    pub fn get_warnings(&self) -> &[String] {
        &self.warnings
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidPackageException,
    inner,
    r"Composer\Package\Loader\InvalidPackageException"
);
