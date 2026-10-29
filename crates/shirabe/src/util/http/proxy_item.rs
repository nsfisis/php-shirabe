//! ref: composer/src/Composer/Util/Http/ProxyItem.php

use crate::util::http::RequestProxy;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, base64_encode, bconcat, legacy_lossy, parse_url, rawurldecode,
    str_replace, strpbrk, strtolower,
};

#[derive(Debug)]
pub struct ProxyItem {
    url: BString,
    safe_url: BString,
    curl_auth: Option<BString>,
    options_proxy: BString,
    options_auth: Option<BString>,
}

impl ProxyItem {
    pub fn new(proxy_url: impl AsRef<[u8]>, env_name: String) -> Result<Self, RuntimeException> {
        let proxy_url = proxy_url.as_ref();
        let syntax_error = format!("unsupported `{}` syntax", env_name);

        if strpbrk(proxy_url, "\r\n\t").is_some() {
            return Err(RuntimeException::new(syntax_error));
        }

        let Some(proxy) = parse_url(proxy_url) else {
            return Err(RuntimeException::new(syntax_error));
        };

        let Some(host) = proxy.host else {
            return Err(RuntimeException::new(format!(
                "unable to find proxy host in {}",
                env_name
            )));
        };

        let scheme = match &proxy.scheme {
            Some(scheme) => bconcat!(strtolower(scheme), "://"),
            None => BString::from("http://"),
        };
        let mut safe = BString::default();

        let mut curl_auth: Option<BString> = None;
        let mut options_auth: Option<BString> = None;

        if let Some(user_raw) = &proxy.user {
            safe = "***".into();
            let mut user = user_raw.clone();
            let mut auth = rawurldecode(user_raw);

            if let Some(pass_raw) = &proxy.pass {
                safe.extend_from_slice(b":***");
                user = bconcat!(user, ":", pass_raw);
                auth = bconcat!(auth, ":", rawurldecode(pass_raw));
            }

            safe.extend_from_slice(b"@");

            if !user.is_empty() {
                curl_auth = Some(user);
                options_auth = Some(bconcat!(
                    "Proxy-Authorization: Basic ",
                    base64_encode(&auth)
                ));
            }
        }

        let port: Option<i64>;

        if let Some(proxy_port) = proxy.port {
            port = Some(proxy_port);
        } else if scheme == "http://" {
            port = Some(80);
        } else if scheme == "https://" {
            port = Some(443);
        } else {
            port = None;
        }

        // We need a port because curl uses 1080 for http. Port 0 is reserved,
        // but is considered valid depending on the PHP or Curl version.
        let port = match port {
            None => {
                return Err(RuntimeException::new(format!(
                    "unable to find proxy port in {}",
                    env_name
                )));
            }
            Some(0) => {
                return Err(RuntimeException::new(format!(
                    "port 0 is reserved in {}",
                    env_name
                )));
            }
            Some(p) => p,
        };

        let url = bconcat!(scheme, host, ":", port);
        let safe_url = bconcat!(scheme, safe, host, ":", port);

        let options_proxy_scheme = str_replace(
            "https://",
            "ssl://",
            str_replace("http://", "tcp://", &scheme),
        );
        let options_proxy = bconcat!(options_proxy_scheme, host, ":", port);

        Ok(Self {
            url,
            safe_url,
            curl_auth,
            options_proxy,
            options_auth,
        })
    }

    pub fn to_request_proxy(&self, scheme: String) -> RequestProxy {
        let mut http_options: IndexMap<BString, PhpMixed> = IndexMap::new();
        http_options.insert("proxy".into(), PhpMixed::string(self.options_proxy.clone()));

        if let Some(ref auth) = self.options_auth {
            http_options.insert("header".into(), PhpMixed::string(auth.clone()));
        }

        if scheme == "http" {
            http_options.insert("request_fulluri".into(), PhpMixed::Bool(true));
        }

        let mut options: IndexMap<String, IndexMap<BString, PhpMixed>> = IndexMap::new();
        options.insert("http".to_string(), http_options);

        RequestProxy::new(
            Some(legacy_lossy(&self.url).into_owned()),
            self.curl_auth
                .as_ref()
                .map(|curl_auth| legacy_lossy(curl_auth).into_owned()),
            Some(options),
            Some(legacy_lossy(&self.safe_url).into_owned()),
        )
    }
}
