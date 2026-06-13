//! ref: composer/src/Composer/Util/ComposerMirror.php

#[derive(Debug)]
pub struct ComposerMirror;

impl ComposerMirror {
    pub fn process_url(
        _mirror_url: &str,
        _package_name: &str,
        _version: &str,
        _reference: Option<&str>,
        _r_type: Option<&str>,
        _pretty_version: Option<&str>,
    ) -> String {
        todo!()
    }

    pub fn process_git_url(
        _mirror: &str,
        _package_name: &str,
        _url: &str,
        _extension: &str,
    ) -> String {
        todo!()
    }

    pub fn process_hg_url(
        _mirror: &str,
        _package_name: &str,
        _url: &str,
        _extension: &str,
    ) -> String {
        todo!()
    }
}
