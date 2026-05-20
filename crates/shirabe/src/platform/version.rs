//! ref: composer/src/Composer/Platform/Version.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::version_compare;

pub struct Version;

impl Version {
    pub fn parse_openssl(openssl_version: &str, is_fips: &mut bool) -> Option<String> {
        *is_fips = false;

        let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
        if !Preg::match_strict_groups3(
            r"^(?P<version>[0-9.]+)(?P<patch>[a-z]{0,2})(?P<suffix>(?:-?(?:dev|pre|alpha|beta|rc|fips)[\d]*)*)(?:-\w+)?(?: \(.+?\))?$",
            openssl_version,
            Some(&mut matches),
        )
        .unwrap_or(false)
        {
            return None;
        }

        let version = matches
            .get(&CaptureKey::ByName("version".to_string()))
            .cloned()
            .unwrap_or_default();
        let patch_str = matches
            .get(&CaptureKey::ByName("patch".to_string()))
            .cloned()
            .unwrap_or_default();
        let suffix_str = matches
            .get(&CaptureKey::ByName("suffix".to_string()))
            .cloned()
            .unwrap_or_default();

        let patch = if version_compare(&version, "3.0.0", "<") {
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
        let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
        if !Preg::match_strict_groups3(
            r"^(?P<major>\d+)(?P<minor>[a-z]*)$",
            libjpeg_version,
            Some(&mut matches),
        )
        .unwrap_or(false)
        {
            return None;
        }

        let major = matches
            .get(&CaptureKey::ByName("major".to_string()))
            .cloned()
            .unwrap_or_default();
        let minor = matches
            .get(&CaptureKey::ByName("minor".to_string()))
            .cloned()
            .unwrap_or_default();
        Some(format!(
            "{}.{}",
            major,
            Self::convert_alpha_version_to_int_version(&minor)
        ))
    }

    pub fn parse_zoneinfo_version(zoneinfo_version: &str) -> Option<String> {
        let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
        if !Preg::match_strict_groups3(
            r"^(?P<year>\d{4})(?P<revision>[a-z]*)$",
            zoneinfo_version,
            Some(&mut matches),
        )
        .unwrap_or(false)
        {
            return None;
        }

        let year = matches
            .get(&CaptureKey::ByName("year".to_string()))
            .cloned()
            .unwrap_or_default();
        let revision = matches
            .get(&CaptureKey::ByName("revision".to_string()))
            .cloned()
            .unwrap_or_default();
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
