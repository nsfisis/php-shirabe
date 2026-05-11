//! ref: composer/src/Composer/Repository/PearRepository.php

use super::array_repository::ArrayRepository;
use shirabe_php_shim::InvalidArgumentException;

#[derive(Debug)]
pub struct PearRepository {
    inner: ArrayRepository,
}

impl PearRepository {
    pub fn new() -> anyhow::Result<Self> {
        Err(anyhow::anyhow!(InvalidArgumentException {
            message: "The PEAR repository has been removed from Composer 2.x".to_string(),
            code: 0,
        }))
    }
}
