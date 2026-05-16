//! ref: composer/src/Composer/Util/ComposerMirror.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::hash;

pub struct ComposerMirror;

impl ComposerMirror {
    pub fn process_url(
        mirror_url: &str,
        package_name: &str,
        version: &str,
        reference: Option<&str>,
        r#type: Option<&str>,
        pretty_version: Option<&str>,
    ) -> String {
        let reference = reference.map(|r| {
            if Preg::is_match(r"^([a-f0-9]*|%reference%)$", r) {
                r.to_string()
            } else {
                hash("md5", r)
            }
        });
        let version = if !version.contains('/') {
            version.to_string()
        } else {
            hash("md5", version)
        };

        let mut from = vec!["%package%", "%version%", "%reference%", "%type%"];
        let mut to: Vec<&str> = vec![
            package_name,
            &version,
            reference.as_deref().unwrap_or(""),
            r#type.unwrap_or(""),
        ];
        if let Some(pv) = pretty_version {
            from.push("%prettyVersion%");
            to.push(pv);
        }

        let url = from
            .iter()
            .zip(to.iter())
            .fold(mirror_url.to_string(), |acc, (f, t)| acc.replace(f, t));
        assert!(!url.is_empty());
        url
    }

    pub fn process_git_url(
        mirror_url: &str,
        package_name: &str,
        url: &str,
        r#type: Option<&str>,
    ) -> String {
        let normalized_url = if let Some(m) = Preg::match_(
            r"^(?:(?:https?|git)://github\.com/|git@github\.com:)([^/]+)/(.+?)(?:\.git)?$",
            url,
        ) {
            format!("gh-{}/{}", m[1], m[2])
        } else if let Some(m) =
            Preg::match_(r"^https://bitbucket\.org/([^/]+)/(.+?)(?:\.git)?/?$", url)
        {
            format!("bb-{}/{}", m[1], m[2])
        } else {
            Preg::replace(r"[^a-z0-9_.-]", "-", url.trim_matches('/'))
        };

        ["%package%", "%normalizedUrl%", "%type%"]
            .iter()
            .zip([package_name, &normalized_url, r#type.unwrap_or("")])
            .fold(mirror_url.to_string(), |acc, (f, t)| acc.replace(f, t))
    }

    pub fn process_hg_url(mirror_url: &str, package_name: &str, url: &str, r#type: &str) -> String {
        Self::process_git_url(mirror_url, package_name, url, Some(r#type))
    }
}
