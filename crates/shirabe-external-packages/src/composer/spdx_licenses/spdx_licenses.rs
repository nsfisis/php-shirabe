use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct SpdxLicenses;

impl SpdxLicenses {
    pub fn new() -> Self {
        todo!()
    }

    pub fn validate(&self, license: &str) -> bool {
        todo!()
    }

    pub fn get_license_by_identifier(&self, identifier: &str) -> Option<PhpMixed> {
        todo!()
    }

    pub fn get_licenses(&self) -> PhpMixed {
        todo!()
    }
}
