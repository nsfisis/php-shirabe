//! ref: composer/src/Composer/Util/Url.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{in_array, parse_url, PhpMixed, PHP_URL_HOST, PHP_URL_PORT};
use crate::config::Config;
use crate::util::github::GitHub;

pub struct Url;

impl Url {
    pub fn update_dist_reference(config: &Config, mut url: String, r#ref: &str) -> String {
        let host = parse_url(&url, PHP_URL_HOST).as_string_opt().map(|s| s.to_string()).unwrap_or_default();

        if host == "api.github.com" || host == "github.com" || host == "www.github.com" {
            if let Some(m) = Preg::match_(r"(?i)^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)/(zip|tar)ball/(.+)$", &url) {
                url = format!("https://api.github.com/repos/{}/{}/{}ball/{}", m.get("1").unwrap_or(&String::new()), m.get("2").unwrap_or(&String::new()), m.get("3").unwrap_or(&String::new()), r#ref);
            } else if let Some(m) = Preg::match_(r"(?i)^https?://(?:www\.)?github\.com/([^/]+)/([^/]+)/archive/.+\.(zip|tar)(?:\.gz)?$", &url) {
                url = format!("https://api.github.com/repos/{}/{}/{}ball/{}", m.get("1").unwrap_or(&String::new()), m.get("2").unwrap_or(&String::new()), m.get("3").unwrap_or(&String::new()), r#ref);
            } else if let Some(m) = Preg::match_(r"(?i)^https?://api\.github\.com/repos/([^/]+)/([^/]+)/(zip|tar)ball(?:/.+)?$", &url) {
                url = format!("https://api.github.com/repos/{}/{}/{}ball/{}", m.get("1").unwrap_or(&String::new()), m.get("2").unwrap_or(&String::new()), m.get("3").unwrap_or(&String::new()), r#ref);
            }
        } else if host == "bitbucket.org" || host == "www.bitbucket.org" {
            if let Some(m) = Preg::match_(r"(?i)^https?://(?:www\.)?bitbucket\.org/([^/]+)/([^/]+)/get/(.+)\.(zip|tar\.gz|tar\.bz2)$", &url) {
                url = format!("https://bitbucket.org/{}/{}/get/{}.{}", m.get("1").unwrap_or(&String::new()), m.get("2").unwrap_or(&String::new()), r#ref, m.get("4").unwrap_or(&String::new()));
            }
        } else if host == "gitlab.com" || host == "www.gitlab.com" {
            if let Some(m) = Preg::match_(r"(?i)^https?://(?:www\.)?gitlab\.com/api/v[34]/projects/([^/]+)/repository/archive\.(zip|tar\.gz|tar\.bz2|tar)\?sha=.+$", &url) {
                url = format!("https://gitlab.com/api/v4/projects/{}/repository/archive.{}?sha={}", m.get("1").unwrap_or(&String::new()), m.get("2").unwrap_or(&String::new()), r#ref);
            }
        } else if in_array(PhpMixed::String(host.clone()), &config.get("github-domains"), true) {
            url = Preg::replace(r"(?i)(/repos/[^/]+/[^/]+/(zip|tar)ball)(?:/.+)?$", &format!("$1/{}", r#ref), url);
        } else if in_array(PhpMixed::String(host.clone()), &config.get("gitlab-domains"), true) {
            url = Preg::replace(r"(?i)(/api/v[34]/projects/[^/]+/repository/archive\.(?:zip|tar\.gz|tar\.bz2|tar)\?sha=).+$", &format!("${{1}}{}", r#ref), url);
        }

        assert!(!url.is_empty());

        url
    }

    pub fn get_origin(config: &Config, url: &str) -> String {
        if url.starts_with("file://") {
            return url.to_string();
        }

        let mut origin = parse_url(url, PHP_URL_HOST).as_string_opt().map(|s| s.to_string()).unwrap_or_default();
        if let Some(port) = parse_url(url, PHP_URL_PORT).as_i64_opt() {
            origin = format!("{}:{}", origin, port);
        }

        if origin.ends_with(".github.com") && origin != "codeload.github.com" {
            return "github.com".to_string();
        }

        if origin == "repo.packagist.org" {
            return "packagist.org".to_string();
        }

        if origin.is_empty() {
            origin = url.to_string();
        }

        // Gitlab can be installed in a non-root context (i.e. gitlab.com/foo). When downloading archives the originUrl
        // is the host without the path, so we look for the registered gitlab-domains matching the host here
        if !origin.contains('/') && !in_array(PhpMixed::String(origin.clone()), &config.get("gitlab-domains"), true) {
            for gitlab_domain in config.get("gitlab-domains").as_vec_string() {
                if !gitlab_domain.is_empty() && gitlab_domain.starts_with(&origin) {
                    return gitlab_domain;
                }
            }
        }

        origin
    }

    pub fn sanitize(url: String) -> String {
        // GitHub repository rename result in redirect locations containing the access_token as GET parameter
        // e.g. https://api.github.com/repositories/9999999999?access_token=github_token
        let url = Preg::replace(r"([&?]access_token=)[^&]+", "$1***", url);

        let url = Preg::replace_callback(
            r"(?i)^(?P<prefix>[a-z0-9]+://)?(?P<user>[^:/\s@]+):(?P<password>[^@\s/]+)@",
            |m| {
                // if the username looks like a long (12char+) hex string, or a modern github token (e.g. ghp_xxx, github_pat_xxx) we obfuscate that
                if Preg::is_match(GitHub::GITHUB_TOKEN_REGEX, m.get("user").map(|s| s.as_str()).unwrap_or("")) {
                    format!("{}***:***@", m.get("prefix").map(|s| s.as_str()).unwrap_or(""))
                } else {
                    format!("{}{}:***@", m.get("prefix").map(|s| s.as_str()).unwrap_or(""), m.get("user").map(|s| s.as_str()).unwrap_or(""))
                }
            },
            url,
        );

        url
    }
}
