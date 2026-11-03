//! ref: composer/src/Composer/Util/ComposerMirror.php

use bstr::{BStr, BString};
use shirabe_php_shim::{
    bconcat, hash, php_regex, preg_is_match, preg_match, preg_replace, str_replace_array, trim2,
};

pub struct ComposerMirror;

impl ComposerMirror {
    pub fn process_url(
        mirror_url: &BStr,
        package_name: &BStr,
        version: &BStr,
        reference: Option<&BStr>,
        r#type: Option<&BStr>,
        pretty_version: Option<&BStr>,
    ) -> BString {
        let reference = reference.map(|r| {
            let truthy = !r.is_empty() && r != "0";
            if !truthy || preg_is_match(php_regex!(r"{^([a-f0-9]*|%reference%)$}"), r) {
                BString::from(r)
            } else {
                hash("md5", r)
            }
        });
        let version = if !version.contains(&b'/') {
            BString::from(version)
        } else {
            hash("md5", version)
        };

        let mut from: Vec<&[u8]> = vec![b"%package%", b"%version%", b"%reference%", b"%type%"];
        let mut to: Vec<&[u8]> = vec![
            package_name,
            &version,
            reference.as_ref().map_or(b"".as_slice(), |r| r.as_slice()),
            &**r#type.unwrap_or_default(),
        ];
        if let Some(pv) = pretty_version {
            from.push(b"%prettyVersion%");
            to.push(pv);
        }

        let url = str_replace_array(&from, &to, mirror_url);
        assert!(!url.is_empty());
        url
    }

    pub fn process_git_url(
        mirror_url: &BStr,
        package_name: &BStr,
        url: &BStr,
        r#type: Option<&BStr>,
    ) -> BString {
        let normalized_url = if let Some(gh_matches) = preg_match(
            php_regex!(
                r"#^(?:(?:https?|git)://github\.com/|git@github\.com:)([^/]+)/(.+?)(?:\.git)?$#"
            ),
            url,
        ) {
            bconcat!(
                "gh-",
                gh_matches.get(1).unwrap_or_default(),
                "/",
                gh_matches.get(2).unwrap_or_default(),
            )
        } else if let Some(bb_matches) = preg_match(
            php_regex!(r"#^https://bitbucket\.org/([^/]+)/(.+?)(?:\.git)?/?$#"),
            url,
        ) {
            bconcat!(
                "bb-",
                bb_matches.get(1).unwrap_or_default(),
                "/",
                bb_matches.get(2).unwrap_or_default(),
            )
        } else {
            preg_replace(php_regex!(r"{[^a-z0-9_.-]}i"), "-", trim2(url, "/"))
        };

        let from: [&[u8]; 3] = [b"%package%", b"%normalizedUrl%", b"%type%"];
        let to: [&[u8]; 3] = [package_name, &normalized_url, &**r#type.unwrap_or_default()];
        str_replace_array(&from, &to, mirror_url)
    }

    pub fn process_hg_url(
        mirror_url: &BStr,
        package_name: &BStr,
        url: &BStr,
        r#type: &BStr,
    ) -> BString {
        Self::process_git_url(mirror_url, package_name, url, Some(r#type))
    }
}
