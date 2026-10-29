//! ref: composer/src/Composer/Util/NoProxyPattern.php

use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    RuntimeException, array_key_exists, bconcat, explode, filter_var_int_with_range, filter_var_ip,
    inet_pton, ltrim2, parse_url, php_regex, preg_split, stripos, strlen, strpbrk, strpos, substr,
    substr_count, utf8_text,
};

/// Tests URLs against NO_PROXY patterns
#[derive(Debug)]
pub struct NoProxyPattern {
    host_names: Vec<BString>,
    rules: IndexMap<i64, Option<UrlData>>,
    noproxy: bool,
}

#[derive(Debug, Clone)]
pub struct UrlData {
    pub host: BString,
    pub name: BString,
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
    pub fn new(pattern: impl AsRef<[u8]>) -> Self {
        let host_names: Vec<BString> = preg_split(php_regex!(r"{[\s,]+}"), pattern.as_ref())
            .into_iter()
            .filter(|host_name| !host_name.is_empty())
            .collect();
        let noproxy = host_names.is_empty() || host_names[0] == "*";
        Self {
            host_names,
            rules: IndexMap::new(),
            noproxy,
        }
    }

    /// Returns true if a URL matches the NO_PROXY pattern
    pub fn test(&mut self, url: impl AsRef<[u8]>) -> anyhow::Result<bool> {
        if self.noproxy {
            return Ok(true);
        }

        let url_data = match self.get_url_data(url.as_ref())? {
            Some(d) => d,
            None => return Ok(false),
        };

        let host_names = self.host_names.clone();
        for (index, host_name) in host_names.iter().enumerate() {
            if self.r#match(index as i64, host_name.as_bstr(), &url_data)? {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Returns false is the url cannot be parsed, otherwise a data object
    fn get_url_data(&self, url: &[u8]) -> anyhow::Result<Option<UrlData>> {
        let parsed = parse_url(url);
        let Some(host_str) = parsed
            .as_ref()
            .and_then(|parsed| parsed.host.clone())
            .filter(|host| !host.is_empty() && host != "0")
        else {
            return Ok(None);
        };

        let mut port = parsed.as_ref().and_then(|parsed| parsed.port);

        if port.is_none_or(|port| port == 0) {
            match parsed
                .as_ref()
                .and_then(|parsed| parsed.scheme.as_ref())
                .map(|scheme| scheme.as_slice())
            {
                Some(b"http") => port = Some(80),
                Some(b"https") => port = Some(443),
                _ => {}
            }
        }

        let port_int = port.unwrap_or(0);
        let host_name = if port_int != 0 {
            bconcat!(host_str, ":", port_int)
        } else {
            host_str
        };
        let (host, port, err) = self.split_host_port(host_name.as_bstr())?;

        let mut ipdata: Option<IpData> = None;
        if err || !self.ip_check_data(host.as_bstr(), &mut ipdata, false)? {
            return Ok(None);
        }

        Ok(Some(self.make_data(host.as_bstr(), port, ipdata)))
    }

    /// Returns true if the url is matched by a rule
    fn r#match(&mut self, index: i64, host_name: &BStr, url: &UrlData) -> anyhow::Result<bool> {
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
            let haystack = substr(&url.name, -strlen(&rule.name), None);
            matched = stripos(&haystack, &rule.name) == Some(0);
        }

        if matched && rule.port != 0 {
            matched = rule.port == url.port;
        }

        Ok(matched)
    }

    /// Returns true if the target ip is in the network range
    fn match_range(&self, network: &IpData, target: &IpData) -> anyhow::Result<bool> {
        let net = network.ip.as_slice();
        let mask = network.netmask.as_deref().unwrap_or_default();
        let ip = target.ip.as_slice();
        if net.is_empty() {
            return Err(RuntimeException::new(format!(
                "Could not parse network IP {}",
                String::from_utf8_lossy(net)
            ))
            .into());
        }
        if mask.is_empty() {
            return Err(RuntimeException::new(format!(
                "Could not parse netmask {}",
                String::from_utf8_lossy(mask)
            ))
            .into());
        }
        if ip.is_empty() {
            return Err(RuntimeException::new(format!(
                "Could not parse target IP {}",
                String::from_utf8_lossy(ip)
            ))
            .into());
        }

        for i in 0..16 {
            let net_byte = net.get(i).copied().unwrap_or(0);
            let mask_byte = mask.get(i).copied().unwrap_or(0);
            let ip_byte = ip.get(i).copied().unwrap_or(0);
            if (net_byte & mask_byte) != (ip_byte & mask_byte) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Finds or creates rule data for a hostname
    fn get_rule(&mut self, index: i64, host_name: &BStr) -> anyhow::Result<Option<UrlData>> {
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
        if err || !self.ip_check_data(host.as_bstr(), &mut ipdata, true)? {
            return Ok(None);
        }

        self.rules
            .insert(index, Some(self.make_data(host.as_bstr(), port, ipdata)));

        Ok(self.rules.get(&index).and_then(|v| v.clone()))
    }

    /// Creates an object containing IP data if the host is an IP address
    fn ip_check_data(
        &self,
        host: &BStr,
        ipdata: &mut Option<IpData>,
        allow_prefix: bool,
    ) -> anyhow::Result<bool> {
        *ipdata = None;
        let mut netmask: Option<Vec<u8>> = None;
        let mut prefix: Option<i64> = None;
        let mut modified = false;

        let mut host = BString::from(host);

        // Check for a CIDR prefix-length
        if strpos(&host, "/").is_some() {
            let parts = explode("/", &host);
            host = parts.first().cloned().unwrap_or_default();
            let prefix_str = parts.get(1).cloned().unwrap_or_default();

            if !allow_prefix || !self.validate_int(prefix_str.as_bstr(), 0, 128) {
                return Ok(false);
            }
            prefix = Some(
                utf8_text(&prefix_str)
                    .and_then(|prefix| prefix.parse().ok())
                    .unwrap_or(0),
            );
            modified = true;
        }

        // See if this is an ip address
        if !filter_var_ip(&host) {
            return Ok(!modified);
        }

        let (mut ip, size) = self.ip_get_addr(host.as_bstr());

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
    fn ip_get_addr(&self, host: &BStr) -> (Vec<u8>, i64) {
        let ip = inet_pton(host).unwrap_or_default();
        let size = ip.len() as i64;
        let mapped = self.ip_map_to_6(&ip, size);

        (mapped, size)
    }

    /// Returns the binary network mask mapped to IPv6
    fn ip_get_mask(&self, prefix: i64, size: i64) -> Vec<u8> {
        let mut mask: Vec<u8> = Vec::new();

        let ones = (prefix as f64 / 8.0).floor() as i64;
        if ones != 0 {
            mask.extend(vec![0xffu8; ones as usize]);
        }

        let remainder = prefix % 8;
        if remainder != 0 {
            mask.push(0xff ^ (0xff >> remainder));
        }

        if (size as usize) > mask.len() {
            mask.resize(size as usize, 0);
        }

        self.ip_map_to_6(&mask, size)
    }

    /// Calculates and returns the network and mask
    fn ip_get_network(
        &self,
        range_ip: &[u8],
        size: i64,
        prefix: i64,
    ) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let netmask = self.ip_get_mask(prefix, size);

        // Get the network from the address and mask
        if netmask.is_empty() {
            return Err(RuntimeException::new(format!(
                "Could not parse netmask {}",
                String::from_utf8_lossy(&netmask)
            ))
            .into());
        }
        if range_ip.is_empty() {
            return Err(RuntimeException::new(format!(
                "Could not parse range IP {}",
                String::from_utf8_lossy(range_ip)
            ))
            .into());
        }

        let mut net: Vec<u8> = Vec::with_capacity(16);
        for i in 0..16 {
            let ip_byte = range_ip.get(i).copied().unwrap_or(0);
            let mask_byte = netmask.get(i).copied().unwrap_or(0);
            net.push(ip_byte & mask_byte);
        }

        Ok((net, netmask))
    }

    /// Maps an IPv4 address to IPv6
    fn ip_map_to_6(&self, binary: &[u8], size: i64) -> Vec<u8> {
        if size == 4 {
            let mut prefix = vec![0u8; 10];
            prefix.extend(vec![0xffu8; 2]);
            prefix.extend_from_slice(binary);
            return prefix;
        }

        binary.to_vec()
    }

    /// Creates a rule data object
    fn make_data(&self, host: &BStr, port: i64, ipdata: Option<IpData>) -> UrlData {
        UrlData {
            host: host.into(),
            name: bconcat!(".", ltrim2(host, ".")),
            port,
            ipdata,
        }
    }

    /// Creates an ip data object
    fn make_ip_data(&self, ip: &[u8], size: i64, netmask: Option<Vec<u8>>) -> IpData {
        IpData {
            ip: ip.to_vec(),
            size,
            netmask,
        }
    }

    /// Splits the hostname into host and port components
    fn split_host_port(&self, host_name: &BStr) -> anyhow::Result<(BString, i64, bool)> {
        // host, port, err
        let error = (BString::default(), 0_i64, true);
        let mut port: i64 = 0;
        let mut ip6 = BString::default();

        let mut host_name = BString::from(host_name);

        // Check for square-bracket notation
        // PHP: if ($hostName[0] === '[')
        if host_name.starts_with(b"[") {
            let index = strpos(&host_name, "]");

            // The smallest ip6 address is ::
            let index = match index {
                None => return Ok(error),
                Some(i) if (i as i64) < 3 => return Ok(error),
                Some(i) => i,
            };

            ip6 = substr(&host_name, 1, Some((index as i64) - 1));
            host_name = substr(&host_name, (index as i64) + 1, None);

            if strpbrk(&host_name, "[]").is_some() || substr_count(&host_name, ":") > 1 {
                return Ok(error);
            }
        }

        if substr_count(&host_name, ":") == 1 {
            let index = strpos(&host_name, ":").unwrap_or(0);
            let port_str = substr(&host_name, (index as i64) + 1, None);
            host_name = substr(&host_name, 0, Some(index as i64));

            if !self.validate_int(port_str.as_bstr(), 1, 65535) {
                return Ok(error);
            }

            port = utf8_text(&port_str)
                .and_then(|port| port.parse().ok())
                .unwrap_or(0);
        }

        let host = bconcat!(ip6, host_name);

        Ok((host, port, false))
    }

    /// Wrapper around filter_var FILTER_VALIDATE_INT
    fn validate_int(&self, int: &BStr, min: i64, max: i64) -> bool {
        filter_var_int_with_range(int, min, max)
    }
}
