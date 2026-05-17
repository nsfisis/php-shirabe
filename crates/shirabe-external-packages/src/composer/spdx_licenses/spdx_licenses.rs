use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct SpdxLicenses;

impl Default for SpdxLicenses {
    fn default() -> Self {
        Self::new()
    }
}

impl SpdxLicenses {
    pub fn new() -> Self {
        todo!()
    }

    pub fn validate(&self, _license: &str) -> bool {
        todo!()
    }

    pub fn get_license_by_identifier(&self, _identifier: &str) -> Option<PhpMixed> {
        todo!()
    }

    pub fn get_licenses(&self) -> PhpMixed {
        todo!()
    }
}
