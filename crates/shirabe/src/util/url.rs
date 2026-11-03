//! ref: composer/src/Composer/Util/Url.php

use crate::config::Config;
use crate::util::GitHub;
use bstr::BString;
use shirabe_php_shim::{
    PhpMixed, bconcat, in_array_strict, parse_url, php_regex, preg_is_match, preg_match,
    preg_replace, preg_replace_callback,
};

pub struct Url;

impl Url {
    pub fn update_dist_reference(
        config: &Config,
        url: impl AsRef<[u8]>,
        r#ref: impl AsRef<[u8]>,
    ) -> BString {
        let mut url = BString::from(url.as_ref());
        let r#ref = r#ref.as_ref();
        let host = parse_url(&url)
            .and_then(|parsed| parsed.host)
            .unwrap_or_default();

        if host == "api.github.com" || host == "github.com" || host == "www.github.com" {
            if let Some(m) = preg_match(
                php_regex!(
                    r"{^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)/(zip|tar)ball/(.+)$}i"
                ),
                &url,
            ) {
                url = bconcat!(
                    "https://api.github.com/repos/",
                    m.get(1).unwrap_or_default(),
                    "/",
                    m.get(2).unwrap_or_default(),
                    "/",
                    m.get(3).unwrap_or_default(),
                    "ball/",
                    r#ref,
                );
            } else if let Some(m) = preg_match(
                php_regex!(
                    r"{^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)/archive/.+\.(zip|tar)(?:\.gz)?$}i"
                ),
                &url,
            ) {
                url = bconcat!(
                    "https://api.github.com/repos/",
                    m.get(1).unwrap_or_default(),
                    "/",
                    m.get(2).unwrap_or_default(),
                    "/",
                    m.get(3).unwrap_or_default(),
                    "ball/",
                    r#ref,
                );
            } else if let Some(m) = preg_match(
                php_regex!(
                    r"{^https?://api\.github\.com/repos/([^/]+)/([^/]+)/(zip|tar)ball(?:/.+)?$}i"
                ),
                &url,
            ) {
                url = bconcat!(
                    "https://api.github.com/repos/",
                    m.get(1).unwrap_or_default(),
                    "/",
                    m.get(2).unwrap_or_default(),
                    "/",
                    m.get(3).unwrap_or_default(),
                    "ball/",
                    r#ref,
                );
            }
        } else if host == "bitbucket.org" || host == "www.bitbucket.org" {
            if let Some(m) = preg_match(
                php_regex!(
                    r"{^https?://(?:www\.)?bitbucket\.org/([^/]+)/([^/]+)/get/(.+)\.(zip|tar\.gz|tar\.bz2)$}i"
                ),
                &url,
            ) {
                url = bconcat!(
                    "https://bitbucket.org/",
                    m.get(1).unwrap_or_default(),
                    "/",
                    m.get(2).unwrap_or_default(),
                    "/get/",
                    r#ref,
                    ".",
                    m.get(4).unwrap_or_default(),
                );
            }
        } else if host == "gitlab.com" || host == "www.gitlab.com" {
            if let Some(m) = preg_match(
                php_regex!(
                    r"{^https?://(?:www\.)?gitlab\.com/api/v[34]/projects/([^/]+)/repository/archive\.(zip|tar\.gz|tar\.bz2|tar)\?sha=.+$}i"
                ),
                &url,
            ) {
                url = bconcat!(
                    "https://gitlab.com/api/v4/projects/",
                    m.get(1).unwrap_or_default(),
                    "/repository/archive.",
                    m.get(2).unwrap_or_default(),
                    "?sha=",
                    r#ref,
                );
            }
        } else if in_array_strict(
            PhpMixed::string(host.clone()),
            config.get("github-domains").values(),
        ) {
            url = preg_replace(
                php_regex!(r"{(/repos/[^/]+/[^/]+/(zip|tar)ball)(?:/.+)?$}i"),
                bconcat!("$1/", r#ref),
                &url,
            );
        } else if in_array_strict(
            PhpMixed::string(host),
            config.get("gitlab-domains").values(),
        ) {
            url = preg_replace(
                php_regex!(
                    r"{(/api/v[34]/projects/[^/]+/repository/archive\.(?:zip|tar\.gz|tar\.bz2|tar)\?sha=).+$}i"
                ),
                bconcat!("${1}", r#ref),
                &url,
            );
        }

        assert!(!url.is_empty());

        url
    }

    pub fn get_origin(config: &Config, url: impl AsRef<[u8]>) -> BString {
        let url = url.as_ref();
        if url.starts_with(b"file://") {
            return url.into();
        }

        let parsed = parse_url(url);
        let mut origin = parsed
            .as_ref()
            .and_then(|parsed| parsed.host.clone())
            .unwrap_or_default();
        if let Some(port) = parsed
            .and_then(|parsed| parsed.port)
            .filter(|port| *port != 0)
        {
            origin = bconcat!(origin, ":", port);
        }

        if origin.ends_with(b".github.com") && origin != "codeload.github.com" {
            return "github.com".into();
        }

        if origin == "repo.packagist.org" {
            return "packagist.org".into();
        }

        if origin.is_empty() {
            origin = url.into();
        }

        // Gitlab can be installed in a non-root context (i.e. gitlab.com/foo). When downloading archives the originUrl
        // is the host without the path, so we look for the registered gitlab-domains matching the host here
        if !origin.contains(&b'/')
            && !in_array_strict(
                PhpMixed::string(origin.clone()),
                config.get("gitlab-domains").values(),
            )
        {
            let gitlab_domains: Vec<BString> = match config.get("gitlab-domains") {
                PhpMixed::List(list) => list
                    .iter()
                    .filter_map(|v| v.as_string().map(BString::from))
                    .collect(),
                _ => vec![],
            };
            for gitlab_domain in gitlab_domains {
                if !gitlab_domain.is_empty() && gitlab_domain.starts_with(&origin) {
                    return gitlab_domain;
                }
            }
        }

        origin
    }

    pub fn sanitize(url: impl AsRef<[u8]>) -> BString {
        // GitHub repository rename result in redirect locations containing the access_token as GET parameter
        // e.g. https://api.github.com/repositories/9999999999?access_token=github_token
        let url = preg_replace(php_regex!(r"{([&?]access_token=)[^&]+}"), "$1***", url);

        preg_replace_callback(
            php_regex!(r"{^(?P<prefix>[a-z0-9]+://)?(?P<user>[^:/\s@]+):(?P<password>[^@\s/]+)@}i"),
            |m| {
                let user = m.name("user").unwrap_or_default();
                let prefix = m.name("prefix").unwrap_or_default();
                // if the username looks like a long (12char+) hex string, or a modern github token (e.g. ghp_xxx, github_pat_xxx) we obfuscate that
                Ok(if preg_is_match(GitHub::GITHUB_TOKEN_REGEX, user) {
                    bconcat!(prefix, "***:***@")
                } else {
                    bconcat!(prefix, user, ":***@")
                })
            },
            &url,
        )
        .expect("the replacement callback cannot fail")
    }
}
