use crate::PhpMixed;
use indexmap::IndexMap;

pub const CURL_VERSION_ZSTD: i64 = 8388608;

pub const CURLOPT_PROXY: i64 = 10004;
pub const CURLOPT_NOPROXY: i64 = 10177;
pub const CURLOPT_PROXYAUTH: i64 = 111;
pub const CURLOPT_PROXYUSERPWD: i64 = 10006;
pub const CURLAUTH_BASIC: i64 = 1;
pub const CURLOPT_PROXY_CAINFO: i64 = 246;
pub const CURLOPT_PROXY_CAPATH: i64 = 247;
pub const CURL_VERSION_HTTPS_PROXY: i64 = 2097152;

pub const CURLM_OK: i64 = 0;
pub const CURLM_BAD_HANDLE: i64 = 1;
pub const CURLM_BAD_EASY_HANDLE: i64 = 2;
pub const CURLM_OUT_OF_MEMORY: i64 = 3;
pub const CURLM_INTERNAL_ERROR: i64 = 4;
pub const CURLM_CALL_MULTI_PERFORM: i64 = -1;

pub const CURLMOPT_PIPELINING: i64 = 3;
pub const CURLMOPT_MAX_HOST_CONNECTIONS: i64 = 7;

pub const CURLSHOPT_SHARE: i64 = 1;
pub const CURL_LOCK_DATA_COOKIE: i64 = 2;
pub const CURL_LOCK_DATA_DNS: i64 = 3;
pub const CURL_LOCK_DATA_SSL_SESSION: i64 = 4;

pub const CURLOPT_URL: i64 = 10002;
pub const CURLOPT_FOLLOWLOCATION: i64 = 52;
pub const CURLOPT_CONNECTTIMEOUT: i64 = 78;
pub const CURLOPT_TIMEOUT: i64 = 13;
pub const CURLOPT_WRITEHEADER: i64 = 10029;
pub const CURLOPT_FILE: i64 = 10001;
pub const CURLOPT_ENCODING: i64 = 10102;
pub const CURLOPT_PROTOCOLS: i64 = 181;
pub const CURLOPT_CUSTOMREQUEST: i64 = 10036;
pub const CURLOPT_POSTFIELDS: i64 = 10015;
pub const CURLOPT_HTTPHEADER: i64 = 10023;
pub const CURLOPT_CAINFO: i64 = 10065;
pub const CURLOPT_CAPATH: i64 = 10097;
pub const CURLOPT_SSL_VERIFYPEER: i64 = 64;
pub const CURLOPT_SSL_VERIFYHOST: i64 = 81;
pub const CURLOPT_SSLCERT: i64 = 10025;
pub const CURLOPT_SSLKEY: i64 = 10087;
pub const CURLOPT_SSLKEYPASSWD: i64 = 10026;
pub const CURLOPT_IPRESOLVE: i64 = 113;
pub const CURLOPT_SHARE: i64 = 10100;
pub const CURLOPT_HTTP_VERSION: i64 = 84;

pub const CURLPROTO_HTTP: i64 = 1;
pub const CURLPROTO_HTTPS: i64 = 2;

pub const CURL_IPRESOLVE_V4: i64 = 1;
pub const CURL_IPRESOLVE_V6: i64 = 2;

pub const CURL_HTTP_VERSION_2_0: i64 = 3;
pub const CURL_HTTP_VERSION_3: i64 = 30;

pub const CURL_VERSION_HTTP2: i64 = 65536;
pub const CURL_VERSION_HTTP3: i64 = 33554432;
pub const CURL_VERSION_LIBZ: i64 = 8;

pub const CURLE_OK: i64 = 0;
pub const CURLE_OPERATION_TIMEDOUT: i64 = 28;

#[derive(Debug)]
pub struct CurlHandle;

#[derive(Debug)]
pub struct CurlMultiHandle;

#[derive(Debug)]
pub struct CurlShareHandle;

pub fn curl_version() -> Option<IndexMap<String, PhpMixed>> {
    todo!()
}

pub fn curl_init() -> CurlHandle {
    todo!()
}

pub fn curl_close(_handle: CurlHandle) {
    todo!()
}

pub fn curl_setopt(_handle: &CurlHandle, _option: i64, _value: PhpMixed) -> bool {
    todo!()
}

pub fn curl_setopt_array(_handle: &CurlHandle, _options: &IndexMap<i64, PhpMixed>) -> bool {
    todo!()
}

pub fn curl_getinfo(_handle: &CurlHandle) -> PhpMixed {
    todo!()
}

pub fn curl_error(_handle: &CurlHandle) -> String {
    todo!()
}

pub fn curl_errno(_handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_strerror(_errornum: i64) -> Option<String> {
    todo!()
}

pub fn curl_multi_init() -> CurlMultiHandle {
    todo!()
}

pub fn curl_multi_setopt(_mh: &CurlMultiHandle, _option: i64, _value: PhpMixed) -> bool {
    todo!()
}

pub fn curl_multi_add_handle(_mh: &CurlMultiHandle, _handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_multi_remove_handle(_mh: &CurlMultiHandle, _handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_multi_exec(_mh: &CurlMultiHandle, _still_running: &mut bool) -> i64 {
    todo!()
}

pub fn curl_multi_select(_mh: &CurlMultiHandle, _timeout: f64) -> i64 {
    todo!()
}

pub fn curl_multi_info_read(_mh: &CurlMultiHandle) -> PhpMixed {
    todo!()
}

pub fn curl_share_init() -> CurlShareHandle {
    todo!()
}

pub fn curl_share_setopt(_sh: &CurlShareHandle, _option: i64, _value: PhpMixed) -> bool {
    todo!()
}

/// Cast a `\CurlHandle` to int (its spl_object_id) as `(int) $curlHandle` in PHP.
pub fn curl_handle_id(_handle: &CurlHandle) -> i64 {
    todo!()
}
