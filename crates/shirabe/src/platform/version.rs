//! ref: composer/src/Composer/Platform/Version.php

use shirabe_php_shim::{CmpOp, php_regex, preg_match, version_compare};

pub struct Version;

impl Version {
    pub fn parse_openssl(openssl_version: &str, is_fips: &mut bool) -> Option<String> {
        *is_fips = false;

        let matches = preg_match(
            php_regex!(
                r"/^(?P<version>[0-9.]+)(?P<patch>[a-z]{0,2})(?P<suffix>(?:-?(?:dev|pre|alpha|beta|rc|fips)[\d]*)*)(?:-\w+)?(?: \(.+?\))?$/"
            ),
            openssl_version,
        )?;

        let version = matches.name("version").unwrap_or_default().to_string();
        let patch_str = matches.name("patch").unwrap_or_default().to_string();
        let suffix_str = matches.name("suffix").unwrap_or_default().to_string();

        let patch = if version_compare(&version, "3.0.0", CmpOp::Lt) {
            format!(
                ".{}",
                Self::convert_alpha_version_to_int_version(&patch_str)
            )
        } else {
            String::new()
        };

        *is_fips = suffix_str.contains("fips");
        let suffix = format!("-{}", suffix_str.trim_start_matches('-'))
            .replace("-fips", "")
            .replace("-pre", "-alpha");

        Some(
            format!("{}{}{}", version, patch, suffix)
                .trim_end_matches('-')
                .to_string(),
        )
    }

    pub fn parse_libjpeg(libjpeg_version: &str) -> Option<String> {
        let matches = preg_match(
            php_regex!(r"/^(?P<major>\d+)(?P<minor>[a-z]*)$/"),
            libjpeg_version,
        )?;

        let major = matches.name("major").unwrap_or_default().to_string();
        let minor = matches.name("minor").unwrap_or_default().to_string();
        Some(format!(
            "{}.{}",
            major,
            Self::convert_alpha_version_to_int_version(&minor)
        ))
    }

    pub fn parse_zoneinfo_version(zoneinfo_version: &str) -> Option<String> {
        let matches = preg_match(
            php_regex!(r"/^(?P<year>\d{4})(?P<revision>[a-z]*)$/"),
            zoneinfo_version,
        )?;

        let year = matches.name("year").unwrap_or_default().to_string();
        let revision = matches.name("revision").unwrap_or_default().to_string();
        Some(format!(
            "{}.{}",
            year,
            Self::convert_alpha_version_to_int_version(&revision)
        ))
    }

    fn convert_alpha_version_to_int_version(alpha: &str) -> i64 {
        let len = alpha.len() as i64;
        let sum: i64 = alpha.bytes().map(|b| b as i64).sum();
        len * (-('a' as i64) + 1) + sum
    }

    pub fn convert_libxpm_version_id(version_id: i64) -> String {
        Self::convert_version_id(version_id, 100)
    }

    pub fn convert_openldap_version_id(version_id: i64) -> String {
        Self::convert_version_id(version_id, 100)
    }

    fn convert_version_id(version_id: i64, base: i64) -> String {
        format!(
            "{}.{}.{}",
            version_id / (base * base),
            (version_id / base) % base,
            version_id % base,
        )
    }
}
