//! ref: composer/src/Composer/Platform/Version.php sha1:d86d2f11211b2d0adb582c1c910d231717dd7dcc

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    CmpOp, bconcat, ltrim2, php_regex, preg_match, rtrim2, strpos, strtr_array, version_compare,
};

pub struct Version;

impl Version {
    pub fn parse_openssl(openssl_version: impl AsRef<[u8]>, is_fips: &mut bool) -> Option<BString> {
        *is_fips = false;

        let matches = preg_match(
            php_regex!(
                r"/^(?P<version>[0-9.]+)(?P<patch>[a-z]{0,2})(?P<suffix>(?:-?(?:dev|pre|alpha|beta|rc|fips)[\d]*)*)(?:-\w+)?(?: \(.+?\))?$/"
            ),
            openssl_version.as_ref(),
        )?;

        let version = matches.name("version").unwrap_or_default();
        let patch_str = matches.name("patch").unwrap_or_default();
        let suffix_str = matches.name("suffix").unwrap_or_default();

        let patch = if version_compare(version, "3.0.0", CmpOp::Lt) {
            bconcat!(".", Self::convert_alpha_version_to_int_version(patch_str))
        } else {
            BString::default()
        };

        *is_fips = strpos(suffix_str, "fips").is_some();
        let suffix = strtr_array(
            bconcat!("-", ltrim2(suffix_str, "-")),
            &IndexMap::from([("-fips", ""), ("-pre", "-alpha")]),
        );

        Some(rtrim2(bconcat!(version, patch, suffix), "-"))
    }

    pub fn parse_libjpeg(libjpeg_version: impl AsRef<[u8]>) -> Option<BString> {
        let matches = preg_match(
            php_regex!(r"/^(?P<major>\d+)(?P<minor>[a-z]*)$/"),
            libjpeg_version.as_ref(),
        )?;

        Some(bconcat!(
            matches.name("major").unwrap_or_default(),
            ".",
            Self::convert_alpha_version_to_int_version(matches.name("minor").unwrap_or_default())
        ))
    }

    pub fn parse_zoneinfo_version(zoneinfo_version: impl AsRef<[u8]>) -> Option<BString> {
        let matches = preg_match(
            php_regex!(r"/^(?P<year>\d{4})(?P<revision>[a-z]*)$/"),
            zoneinfo_version.as_ref(),
        )?;

        Some(bconcat!(
            matches.name("year").unwrap_or_default(),
            ".",
            Self::convert_alpha_version_to_int_version(
                matches.name("revision").unwrap_or_default()
            )
        ))
    }

    fn convert_alpha_version_to_int_version(alpha: &BStr) -> i64 {
        let len = alpha.len() as i64;
        let sum: i64 = alpha.iter().map(|&b| b as i64).sum();
        len * (-('a' as i64) + 1) + sum
    }

    pub fn convert_libxpm_version_id(version_id: i64) -> BString {
        Self::convert_version_id(version_id, 100)
    }

    pub fn convert_openldap_version_id(version_id: i64) -> BString {
        Self::convert_version_id(version_id, 100)
    }

    fn convert_version_id(version_id: i64, base: i64) -> BString {
        bconcat!(
            version_id / (base * base),
            ".",
            (version_id / base) % base,
            ".",
            version_id % base,
        )
    }
}
