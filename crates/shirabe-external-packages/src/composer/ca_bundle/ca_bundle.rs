use crate::psr::log::logger_interface::LoggerInterface;

#[derive(Debug)]
pub struct CaBundle;

impl CaBundle {
    pub fn is_openssl_parse_safe() -> bool {
        todo!()
    }

    pub fn get_system_ca_root_bundle_path(_logger: Option<&dyn LoggerInterface>) -> String {
        todo!()
    }

    pub fn validate_ca_file(_ca_file: &str, _logger: Option<&dyn LoggerInterface>) -> bool {
        todo!()
    }

    pub fn get_bundled_ca_bundle_path() -> String {
        todo!()
    }
}
