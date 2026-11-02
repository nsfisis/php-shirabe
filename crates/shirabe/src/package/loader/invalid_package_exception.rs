//! ref: composer/src/Composer/Package/Loader/InvalidPackageException.php

use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{Exception, PhpMixed, bconcat, implode};

#[derive(Debug)]
pub struct InvalidPackageException {
    inner: Exception,
    errors: Vec<BString>,
    warnings: Vec<BString>,
    data: IndexMap<BString, PhpMixed>,
}

impl InvalidPackageException {
    #[track_caller]
    pub fn new(
        errors: Vec<BString>,
        warnings: Vec<BString>,
        data: IndexMap<BString, PhpMixed>,
    ) -> Self {
        let message = bconcat!(
            "Invalid package information: \n",
            implode(
                "\n",
                &errors
                    .iter()
                    .chain(warnings.iter())
                    .cloned()
                    .collect::<Vec<_>>()
            )
        );
        Self {
            inner: Exception::new(message),
            errors,
            warnings,
            data,
        }
    }

    pub fn get_data(&self) -> &IndexMap<BString, PhpMixed> {
        &self.data
    }

    pub fn get_errors(&self) -> &[BString] {
        &self.errors
    }

    pub fn get_warnings(&self) -> &[BString] {
        &self.warnings
    }
}

shirabe_php_shim::impl_php_exception!(
    InvalidPackageException,
    inner,
    r"Composer\Package\Loader\InvalidPackageException"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_keeps_the_bytes_of_errors_and_warnings() {
        let exception = InvalidPackageException::new(
            vec![BString::from(b"name : caf\xE9".as_slice())],
            vec![BString::from(b"type : \xFF".as_slice())],
            IndexMap::new(),
        );
        assert_eq!(
            exception.get_message(),
            b"Invalid package information: \nname : caf\xE9\ntype : \xFF".as_slice()
        );
    }
}
