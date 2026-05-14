//! ref: composer/src/Composer/Util/Http/ProxyItem.php

use indexmap::IndexMap;
use shirabe_php_shim::{
    base64_encode, parse_url_all, rawurldecode, strpbrk, PhpMixed, RuntimeException,
};
use crate::util::http::request_proxy::RequestProxy;

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
            return Err(RuntimeException { message: syntax_error, code: 0 });
        }

        let proxy_parsed = parse_url_all(&proxy_url);
        let proxy = match proxy_parsed.as_array() {
            None => return Err(RuntimeException { message: syntax_error, code: 0 }),
            Some(a) => a.clone(),
        };

        if !proxy.contains_key("host") {
            return Err(RuntimeException {
                message: format!("unable to find proxy host in {}", env_name),
                code: 0,
            });
        }

        let scheme = if proxy.contains_key("scheme") {
            format!("{}://", proxy["scheme"].as_string().unwrap_or("").to_lowercase())
        } else {
            "http://".to_string()
        };
        let mut safe = String::new();

        let mut curl_auth: Option<String> = None;
        let mut options_auth: Option<String> = None;

        if proxy.contains_key("user") {
            safe = "***".to_string();
            let user_raw = proxy["user"].as_string().unwrap_or("");
            let auth_raw = rawurldecode(user_raw);

            let mut user = user_raw.to_string();
            let mut auth = auth_raw;

            if proxy.contains_key("pass") {
                let pass_raw = proxy["pass"].as_string().unwrap_or("");
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

        let host = proxy["host"].as_string().unwrap_or("").to_string();
        let port: Option<i64>;

        if proxy.contains_key("port") {
            port = proxy["port"].as_int();
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
                return Err(RuntimeException {
                    message: format!("unable to find proxy port in {}", env_name),
                    code: 0,
                })
            }
            Some(0) => {
                return Err(RuntimeException {
                    message: format!("port 0 is reserved in {}", env_name),
                    code: 0,
                })
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
        let mut http_options: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        http_options.insert(
            "proxy".to_string(),
            Box::new(PhpMixed::String(self.options_proxy.clone())),
        );

        if let Some(ref auth) = self.options_auth {
            http_options.insert(
                "header".to_string(),
                Box::new(PhpMixed::String(auth.clone())),
            );
        }

        if scheme == "http" {
            http_options.insert(
                "request_fulluri".to_string(),
                Box::new(PhpMixed::Bool(true)),
            );
        }

        let mut options: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        options.insert("http".to_string(), Box::new(PhpMixed::Array(http_options)));

        RequestProxy::new(
            Some(self.url.clone()),
            self.curl_auth.clone(),
            Some(options),
            Some(self.safe_url.clone()),
        )
    }
}
