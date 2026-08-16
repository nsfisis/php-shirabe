//! ref: composer/src/Composer/Util/Http/ProxyItem.php

use crate::util::http::RequestProxy;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, base64_encode, parse_url, rawurldecode, strpbrk,
};

#[derive(Debug)]
pub struct ProxyItem {
    url: String,
    safe_url: String,
    curl_auth: Option<String>,
    options_proxy: String,
    options_auth: Option<String>,
}

impl ProxyItem {
    pub fn new(proxy_url: String, env_name: String) -> Result<Self, RuntimeException> {
        let syntax_error = format!("unsupported `{}` syntax", env_name);

        if strpbrk(&proxy_url, "\r\n\t").is_some() {
            return Err(RuntimeException::new(syntax_error));
        }

        let Some(proxy) = parse_url(&proxy_url) else {
            return Err(RuntimeException::new(syntax_error));
        };

        let Some(host) = proxy.host else {
            return Err(RuntimeException::new(format!(
                "unable to find proxy host in {}",
                env_name
            )));
        };

        let scheme = match &proxy.scheme {
            Some(scheme) => format!("{}://", scheme.to_lowercase()),
            None => "http://".to_string(),
        };
        let mut safe = String::new();

        let mut curl_auth: Option<String> = None;
        let mut options_auth: Option<String> = None;

        if let Some(user_raw) = &proxy.user {
            safe = "***".to_string();
            let auth_raw = rawurldecode(user_raw);

            let mut user = user_raw.clone();
            let mut auth = auth_raw;

            if let Some(pass_raw) = &proxy.pass {
                safe += ":***";
                user += &format!(":{}", pass_raw);
                auth += &format!(":{}", rawurldecode(pass_raw));
            }

            safe += "@";

            if !user.is_empty() {
                curl_auth = Some(user);
                options_auth = Some(format!(
                    "Proxy-Authorization: Basic {}",
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

        let url = format!("{}{}:{}", scheme, host, port);
        let safe_url = format!("{}{}{}:{}", scheme, safe, host, port);

        let options_proxy_scheme = scheme
            .replace("http://", "tcp://")
            .replace("https://", "ssl://");
        let options_proxy = format!("{}{}:{}", options_proxy_scheme, host, port);

        Ok(Self {
            url,
            safe_url,
            curl_auth,
            options_proxy,
            options_auth,
        })
    }

    pub fn to_request_proxy(&self, scheme: String) -> RequestProxy {
        let mut http_options: IndexMap<String, PhpMixed> = IndexMap::new();
        http_options.insert(
            "proxy".to_string(),
            PhpMixed::String(self.options_proxy.clone()),
        );

        if let Some(ref auth) = self.options_auth {
            http_options.insert("header".to_string(), PhpMixed::String(auth.clone()));
        }

        if scheme == "http" {
            http_options.insert("request_fulluri".to_string(), PhpMixed::Bool(true));
        }

        let mut options: IndexMap<String, IndexMap<String, PhpMixed>> = IndexMap::new();
        options.insert("http".to_string(), http_options);

        RequestProxy::new(
            Some(self.url.clone()),
            self.curl_auth.clone(),
            Some(options),
            Some(self.safe_url.clone()),
        )
    }
}
