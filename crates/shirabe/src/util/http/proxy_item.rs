//! ref: composer/src/Composer/Util/Http/ProxyItem.php sha1:fdf304362154e79a965509a498ad52d603efdfea

use crate::util::http::RequestProxy;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, base64_encode, bformat, parse_url, rawurldecode, str_replace,
    strpbrk, strtolower,
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
    pub fn new(proxy_url: impl AsRef<[u8]>, env_name: BString) -> Result<Self, RuntimeException> {
        let proxy_url = proxy_url.as_ref();
        let syntax_error = bformat!("unsupported `{env_name}` syntax");

        if strpbrk(proxy_url, "\r\n\t").is_some() {
            return Err(RuntimeException::new(syntax_error));
        }

        let Some(proxy) = parse_url(proxy_url) else {
            return Err(RuntimeException::new(syntax_error));
        };

        let Some(host) = proxy.host else {
            return Err(RuntimeException::new(bformat!(
                "unable to find proxy host in {env_name}"
            )));
        };

        let scheme = match &proxy.scheme {
            Some(scheme) => bformat!("{}://", strtolower(scheme)),
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
                user = bformat!("{user}:{pass_raw}");
                auth = bformat!("{auth}:{}", rawurldecode(pass_raw));
            }

            safe.extend_from_slice(b"@");

            if !user.is_empty() {
                curl_auth = Some(user);
                options_auth = Some(bformat!(
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
                return Err(RuntimeException::new(bformat!(
                    "unable to find proxy port in {env_name}"
                )));
            }
            Some(0) => {
                return Err(RuntimeException::new(bformat!(
                    "port 0 is reserved in {env_name}"
                )));
            }
            Some(p) => p,
        };

        let url = bformat!("{scheme}{host}:{port}");
        let safe_url = bformat!("{scheme}{safe}{host}:{port}");

        let options_proxy_scheme = str_replace(
            "https://",
            "ssl://",
            str_replace("http://", "tcp://", &scheme),
        );
        let options_proxy = bformat!("{options_proxy_scheme}{host}:{port}");

        Ok(Self {
            url,
            safe_url,
            curl_auth,
            options_proxy,
            options_auth,
        })
    }

    pub fn to_request_proxy(&self, scheme: impl AsRef<[u8]>) -> RequestProxy {
        let mut http_options: IndexMap<BString, PhpMixed> = IndexMap::new();
        http_options.insert("proxy".into(), PhpMixed::string(self.options_proxy.clone()));

        if let Some(ref auth) = self.options_auth {
            http_options.insert("header".into(), PhpMixed::string(auth.clone()));
        }

        if scheme.as_ref() == b"http" {
            http_options.insert("request_fulluri".into(), PhpMixed::Bool(true));
        }

        let mut options: IndexMap<BString, IndexMap<BString, PhpMixed>> = IndexMap::new();
        options.insert(BString::from("http"), http_options);

        RequestProxy::new(
            Some(self.url.clone()),
            self.curl_auth.clone(),
            Some(options),
            Some(self.safe_url.clone()),
        )
    }
}
