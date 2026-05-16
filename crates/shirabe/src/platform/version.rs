//! ref: composer/src/Composer/Platform/Version.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::version_compare;

pub struct Version;

impl Version {
    pub fn parse_openssl(openssl_version: &str, is_fips: &mut bool) -> Option<String> {
        *is_fips = false;

        let matches = Preg::match_strict_groups(
            r"^(?P<version>[0-9.]+)(?P<patch>[a-z]{0,2})(?P<suffix>(?:-?(?:dev|pre|alpha|beta|rc|fips)[\d]*)*)(?:-\w+)?(?: \(.+?\))?$",
            openssl_version,
        )?;

        let patch = if version_compare(&matches["version"], "3.0.0", "<") {
            format!(
                ".{}",
                Self::convert_alpha_version_to_int_version(&matches["patch"])
            )
        } else {
            String::new()
        };

        *is_fips = matches["suffix"].contains("fips");
        let suffix = format!("-{}", matches["suffix"].trim_start_matches('-'))
            .replace("-fips", "")
            .replace("-pre", "-alpha");

        Some(
            format!("{}{}{}", matches["version"], patch, suffix)
                .trim_end_matches('-')
                .to_string(),
        )
    }

    pub fn parse_libjpeg(libjpeg_version: &str) -> Option<String> {
        let matches =
            Preg::match_strict_groups(r"^(?P<major>\d+)(?P<minor>[a-z]*)$", libjpeg_version)?;

        Some(format!(
            "{}.{}",
            matches["major"],
            Self::convert_alpha_version_to_int_version(&matches["minor"])
        ))
    }

    pub fn parse_zoneinfo_version(zoneinfo_version: &str) -> Option<String> {
        let matches =
            Preg::match_strict_groups(r"^(?P<year>\d{4})(?P<revision>[a-z]*)$", zoneinfo_version)?;

        Some(format!(
            "{}.{}",
            matches["year"],
            Self::convert_alpha_version_to_int_version(&matches["revision"])
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
