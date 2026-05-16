//! ref: composer/src/Composer/Util/NoProxyPattern.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    array_key_exists, chr, empty, explode, filter_var, filter_var_with_options, floor, inet_pton,
    ltrim, parse_url, str_pad, str_repeat, stripos, strlen, strpbrk, strpos, substr, substr_count,
    unpack, PhpMixed, RuntimeException, FILTER_VALIDATE_INT, FILTER_VALIDATE_IP, PHP_URL_HOST,
    PHP_URL_PORT, PHP_URL_SCHEME,
};

/// Tests URLs against NO_PROXY patterns
#[derive(Debug)]
pub struct NoProxyPattern {
    /// @var string[]
    pub(crate) host_names: Vec<String>,
    /// @var (null|object)[]
    pub(crate) rules: IndexMap<i64, Option<UrlData>>,
    /// @var bool
    pub(crate) noproxy: bool,
}

#[derive(Debug, Clone)]
pub struct UrlData {
    pub host: String,
    pub name: String,
    pub port: i64,
    pub ipdata: Option<IpData>,
}

#[derive(Debug, Clone)]
pub struct IpData {
    pub ip: Vec<u8>,
    pub size: i64,
    pub netmask: Option<Vec<u8>>,
}

impl NoProxyPattern {
    /// @param string $pattern NO_PROXY pattern
    pub fn new(pattern: &str) -> Self {
        // PHP: Preg::split('{[\s,]+}', $pattern, -1, PREG_SPLIT_NO_EMPTY)
        let host_names = Preg::split(r"{[\s,]+}", pattern);
        let noproxy = host_names.is_empty() || host_names[0] == "*";
        Self {
            host_names,
            rules: IndexMap::new(),
            noproxy,
        }
    }

    /// Returns true if a URL matches the NO_PROXY pattern
    pub fn test(&mut self, url: &str) -> Result<bool> {
        if self.noproxy {
            return Ok(true);
        }

        let url_data = match self.get_url_data(url)? {
            Some(d) => d,
            None => return Ok(false),
        };

        let host_names = self.host_names.clone();
        for (index, host_name) in host_names.iter().enumerate() {
            if self.r#match(index as i64, host_name, &url_data)? {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Returns false is the url cannot be parsed, otherwise a data object
    ///
    /// @return bool|stdClass
    pub(crate) fn get_url_data(&self, url: &str) -> Result<Option<UrlData>> {
        let host = parse_url(url, PHP_URL_HOST);
        if empty(&host) {
            return Ok(None);
        }
        let host_str = host.as_string().unwrap_or("").to_string();

        let mut port_mixed = parse_url(url, PHP_URL_PORT);

        if empty(&port_mixed) {
            match parse_url(url, PHP_URL_SCHEME).as_string() {
                Some("http") => port_mixed = PhpMixed::Int(80),
                Some("https") => port_mixed = PhpMixed::Int(443),
                _ => {}
            }
        }

        let port_int = port_mixed.as_int().unwrap_or(0);
        let host_name = format!(
            "{}{}",
            host_str,
            if port_int != 0 {
                format!(":{}", port_int)
            } else {
                String::new()
            },
        );
        let (host, port, err) = self.split_host_port(&host_name)?;

        let mut ipdata: Option<IpData> = None;
        if err || !self.ip_check_data(&host, &mut ipdata, false)? {
            return Ok(None);
        }

        Ok(Some(self.make_data(&host, port, ipdata)))
    }

    /// Returns true if the url is matched by a rule
    pub(crate) fn r#match(
        &mut self,
        index: i64,
        host_name: &str,
        url: &UrlData,
    ) -> Result<bool> {
        let rule = match self.get_rule(index, host_name)? {
            Some(r) => r,
            None => {
                // Data must have been misformatted
                return Ok(false);
            }
        };

        let mut matched;
        if let Some(rule_ipdata) = &rule.ipdata {
            // Match ipdata first
            let url_ipdata = match &url.ipdata {
                Some(d) => d,
                None => return Ok(false),
            };

            if rule_ipdata.netmask.is_some() {
                return self.match_range(rule_ipdata, url_ipdata);
            }

            matched = rule_ipdata.ip == url_ipdata.ip;
        } else {
            // Match host and port
            let haystack = substr(&url.name, -(strlen(&rule.name) as i64), None);
            matched = stripos(&haystack, &rule.name) == Some(0);
        }

        if matched && rule.port != 0 {
            matched = rule.port == url.port;
        }

        Ok(matched)
    }

    /// Returns true if the target ip is in the network range
    pub(crate) fn match_range(&self, network: &IpData, target: &IpData) -> Result<bool> {
        let net = unpack("C*", &network.ip);
        let mask = unpack(
            "C*",
            network.netmask.as_deref().unwrap_or_default(),
        );
        let ip = unpack("C*", &target.ip);
        let net = match net {
            Some(n) => n,
            None => {
                return Err(RuntimeException {
                    message: format!(
                        "Could not parse network IP {}",
                        String::from_utf8_lossy(&network.ip)
                    ),
                    code: 0,
                }
                .into());
            }
        };
        let mask = match mask {
            Some(m) => m,
            None => {
                return Err(RuntimeException {
                    message: format!(
                        "Could not parse netmask {}",
                        String::from_utf8_lossy(network.netmask.as_deref().unwrap_or_default())
                    ),
                    code: 0,
                }
                .into());
            }
        };
        let ip = match ip {
            Some(i) => i,
            None => {
                return Err(RuntimeException {
                    message: format!(
                        "Could not parse target IP {}",
                        String::from_utf8_lossy(&target.ip)
                    ),
                    code: 0,
                }
                .into());
            }
        };

        // PHP: for ($i = 1; $i < 17; ++$i)
        for i in 1..17 {
            let net_byte = net
                .get(&i.to_string())
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            let mask_byte = mask
                .get(&i.to_string())
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            let ip_byte = ip
                .get(&i.to_string())
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            if (net_byte & mask_byte) != (ip_byte & mask_byte) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Finds or creates rule data for a hostname
    ///
    /// @return null|stdClass Null if the hostname is invalid
    fn get_rule(&mut self, index: i64, host_name: &str) -> Result<Option<UrlData>> {
        if array_key_exists(&index.to_string(), &{
            let mut m: IndexMap<String, ()> = IndexMap::new();
            for k in self.rules.keys() {
                m.insert(k.to_string(), ());
            }
            m
        }) {
            return Ok(self.rules.get(&index).and_then(|v| v.clone()));
        }

        self.rules.insert(index, None);
        let (host, port, err) = self.split_host_port(host_name)?;

        let mut ipdata: Option<IpData> = None;
        if err || !self.ip_check_data(&host, &mut ipdata, true)? {
            return Ok(None);
        }

        self.rules
            .insert(index, Some(self.make_data(&host, port, ipdata)));

        Ok(self.rules.get(&index).and_then(|v| v.clone()))
    }

    /// Creates an object containing IP data if the host is an IP address
    ///
    /// @param null|stdClass $ipdata      Set by method if IP address found
    /// @param bool          $allowPrefix Whether a CIDR prefix-length is expected
    ///
    /// @return bool False if the host contains invalid data
    fn ip_check_data(
        &self,
        host: &str,
        ipdata: &mut Option<IpData>,
        allow_prefix: bool,
    ) -> Result<bool> {
        *ipdata = None;
        let mut netmask: Option<Vec<u8>> = None;
        let mut prefix: Option<i64> = None;
        let mut modified = false;

        let mut host = host.to_string();

        // Check for a CIDR prefix-length
        if strpos(&host, "/").is_some() {
            let parts = explode("/", &host);
            host = parts.get(0).cloned().unwrap_or_default();
            let prefix_str = parts.get(1).cloned().unwrap_or_default();

            if !allow_prefix || !self.validate_int(&prefix_str, 0, 128) {
                return Ok(false);
            }
            prefix = Some(prefix_str.parse().unwrap_or(0));
            modified = true;
        }

        // See if this is an ip address
        if !filter_var(&host, FILTER_VALIDATE_IP) {
            return Ok(!modified);
        }

        let (mut ip, size) = self.ip_get_addr(&host);

        if let Some(prefix) = prefix {
            // Check for a valid prefix
            if prefix > size * 8 {
                return Ok(false);
            }

            let (new_ip, new_netmask) = self.ip_get_network(&ip, size, prefix)?;
            ip = new_ip;
            netmask = Some(new_netmask);
        }

        *ipdata = Some(self.make_ip_data(&ip, size, netmask));

        Ok(true)
    }

    /// Returns an array of the IP in_addr and its byte size
    ///
    /// IPv4 addresses are always mapped to IPv6, which simplifies handling
    /// and comparison.
    ///
    /// @return mixed[] in_addr, size
    fn ip_get_addr(&self, host: &str) -> (Vec<u8>, i64) {
        let ip = inet_pton(host).unwrap_or_default();
        let size = ip.len() as i64;
        let mapped = self.ip_map_to_6(&ip, size);

        (mapped, size)
    }

    /// Returns the binary network mask mapped to IPv6
    ///
    /// @param int $prefix CIDR prefix-length
    /// @param int $size   Byte size of in_addr
    fn ip_get_mask(&self, prefix: i64, size: i64) -> Vec<u8> {
        let mut mask = String::new();

        let ones = floor(prefix as f64 / 8.0) as i64;
        if ones != 0 {
            mask = str_repeat(&chr(255), ones as usize);
        }

        let remainder = prefix % 8;
        if remainder != 0 {
            mask.push_str(&chr(0xff ^ (0xff >> remainder)));
        }

        let mask = str_pad(&mask, size as usize, &chr(0), shirabe_php_shim::STR_PAD_RIGHT);

        self.ip_map_to_6(mask.as_bytes(), size)
    }

    /// Calculates and returns the network and mask
    ///
    /// @param string $rangeIp IP in_addr
    /// @param int    $size    Byte size of in_addr
    /// @param int    $prefix  CIDR prefix-length
    ///
    /// @return string[] network in_addr, binary mask
    fn ip_get_network(
        &self,
        range_ip: &[u8],
        size: i64,
        prefix: i64,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        let netmask = self.ip_get_mask(prefix, size);

        // Get the network from the address and mask
        let mask = unpack("C*", &netmask);
        let ip = unpack("C*", range_ip);
        let mut net: Vec<u8> = vec![];
        let mask = match mask {
            Some(m) => m,
            None => {
                return Err(RuntimeException {
                    message: format!(
                        "Could not parse netmask {}",
                        String::from_utf8_lossy(&netmask)
                    ),
                    code: 0,
                }
                .into());
            }
        };
        let ip = match ip {
            Some(i) => i,
            None => {
                return Err(RuntimeException {
                    message: format!(
                        "Could not parse range IP {}",
                        String::from_utf8_lossy(range_ip)
                    ),
                    code: 0,
                }
                .into());
            }
        };

        for i in 1..17 {
            let ip_byte = ip
                .get(&i.to_string())
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            let mask_byte = mask
                .get(&i.to_string())
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            // PHP: $net .= chr($ip[$i] & $mask[$i]);
            net.extend(chr((ip_byte & mask_byte) as u8).as_bytes());
        }

        Ok((net, netmask))
    }

    /// Maps an IPv4 address to IPv6
    ///
    /// @param string $binary in_addr
    /// @param int    $size   Byte size of in_addr
    ///
    /// @return string Mapped or existing in_addr
    fn ip_map_to_6(&self, binary: &[u8], size: i64) -> Vec<u8> {
        if size == 4 {
            let mut prefix = str_repeat(&chr(0), 10).into_bytes();
            prefix.extend(str_repeat(&chr(255), 2).into_bytes());
            prefix.extend_from_slice(binary);
            return prefix;
        }

        binary.to_vec()
    }

    /// Creates a rule data object
    fn make_data(&self, host: &str, port: i64, ipdata: Option<IpData>) -> UrlData {
        UrlData {
            host: host.to_string(),
            name: format!(".{}", ltrim(host, Some("."))),
            port,
            ipdata,
        }
    }

    /// Creates an ip data object
    ///
    /// @param string      $ip      in_addr
    /// @param int         $size    Byte size of in_addr
    /// @param null|string $netmask Network mask
    fn make_ip_data(&self, ip: &[u8], size: i64, netmask: Option<Vec<u8>>) -> IpData {
        IpData {
            ip: ip.to_vec(),
            size,
            netmask,
        }
    }

    /// Splits the hostname into host and port components
    ///
    /// @return mixed[] host, port, if there was error
    fn split_host_port(&self, host_name: &str) -> Result<(String, i64, bool)> {
        // host, port, err
        let error = (String::new(), 0_i64, true);
        let mut port: i64 = 0;
        let mut ip6 = String::new();

        let mut host_name = host_name.to_string();

        // Check for square-bracket notation
        // PHP: if ($hostName[0] === '[')
        if host_name.chars().next() == Some('[') {
            let index = strpos(&host_name, "]");

            // The smallest ip6 address is ::
            let index = match index {
                None => return Ok(error),
                Some(i) if (i as i64) < 3 => return Ok(error),
                Some(i) => i,
            };

            ip6 = substr(&host_name, 1, Some((index as i64) - 1));
            host_name = substr(&host_name, (index as i64) + 1, None);

            if strpbrk(&host_name, "[]").is_some()
                || substr_count(&host_name, ":") > 1
            {
                return Ok(error);
            }
        }

        if substr_count(&host_name, ":") == 1 {
            let index = strpos(&host_name, ":").unwrap_or(0);
            let port_str = substr(&host_name, (index as i64) + 1, None);
            host_name = substr(&host_name, 0, Some(index as i64));

            if !self.validate_int(&port_str, 1, 65535) {
                return Ok(error);
            }

            port = port_str.parse().unwrap_or(0);
        }

        let host = format!("{}{}", ip6, host_name);

        Ok((host, port, false))
    }

    /// Wrapper around filter_var FILTER_VALIDATE_INT
    fn validate_int(&self, int: &str, min: i64, max: i64) -> bool {
        let mut options: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut inner: IndexMap<String, PhpMixed> = IndexMap::new();
        inner.insert("min_range".to_string(), PhpMixed::Int(min));
        inner.insert("max_range".to_string(), PhpMixed::Int(max));
        options.insert(
            "options".to_string(),
            PhpMixed::Array(
                inner
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        );

        !matches!(
            filter_var_with_options(int, FILTER_VALIDATE_INT, &options),
            PhpMixed::Bool(false)
        )
    }
}
