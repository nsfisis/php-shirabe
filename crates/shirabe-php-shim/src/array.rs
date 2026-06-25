use crate::PhpMixed;
use crate::php_to_string;
use indexmap::IndexMap;

pub fn array_values<V: Clone>(_array: &IndexMap<String, V>) -> Vec<V> {
    _array.values().cloned().collect()
}

pub fn array_keys<V>(_array: &IndexMap<String, V>) -> Vec<String> {
    _array.keys().cloned().collect()
}

pub fn array_push(_array: &mut Vec<String>, _value: String) -> i64 {
    _array.push(_value);
    _array.len() as i64
}

pub fn array_search_in_vec(_needle: &str, _haystack: &[String]) -> Option<usize> {
    _haystack.iter().position(|s| s.as_str() == _needle)
}

pub fn array_map_str_fn<F: Fn(&str) -> String>(_callback: F, _array: &[String]) -> Vec<String> {
    _array.iter().map(|s| _callback(s)).collect()
}

pub fn array_slice_mixed(value: &PhpMixed, offset: i64, length: Option<i64>) -> PhpMixed {
    match value {
        PhpMixed::List(items) => {
            let (start, end) = php_slice_bounds(items.len() as i64, offset, length);
            PhpMixed::List(items[start..end].to_vec())
        }
        PhpMixed::Array(map) => {
            let (start, end) = php_slice_bounds(map.len() as i64, offset, length);
            PhpMixed::Array(
                map.iter()
                    .skip(start)
                    .take(end - start)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            )
        }
        _ => panic!("array_slice(): Argument #1 ($array) must be of type array"),
    }
}

pub fn array_slice_strs(value: &[String], offset: i64, length: Option<i64>) -> Vec<String> {
    let (start, end) = php_slice_bounds(value.len() as i64, offset, length);
    value[start..end].to_vec()
}

pub fn array_fill_keys(keys: PhpMixed, value: PhpMixed) -> PhpMixed {
    let entries: Vec<&PhpMixed> = match &keys {
        PhpMixed::List(items) => items.iter().collect(),
        PhpMixed::Array(map) => map.values().collect(),
        _ => panic!("array_fill_keys(): Argument #1 ($keys) must be of type array"),
    };
    let mut result: IndexMap<String, PhpMixed> = IndexMap::new();
    for key in entries {
        result.insert(php_to_string(key), value.clone());
    }
    PhpMixed::Array(result)
}

/// PHP `array_merge`.
///
/// Must reproduce PHP's mixed integer/string key semantics:
/// - string keys: a later array's value overwrites an earlier one, keeping the
///   earlier key's position;
/// - integer-like keys ("0","1",...): values are appended and renumbered
///   sequentially across all inputs (they are NOT overwritten by key).
///
/// A naive per-entry `IndexMap::insert` is INCORRECT for inputs that mix string
/// and integer keys (e.g. an AliasPackage's provides/replaces, where
/// self.version expansion appends links under "0","1",... keys). See the typed
/// [`array_merge_map`] variant used by such call sites.
pub fn array_merge(array1: PhpMixed, array2: PhpMixed) -> PhpMixed {
    let mut result: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut next_int: i64 = 0;
    for array in [array1, array2] {
        match array {
            PhpMixed::List(items) => {
                for value in items {
                    result.insert(next_int.to_string(), value);
                    next_int += 1;
                }
            }
            PhpMixed::Array(map) => {
                for (key, value) in map {
                    if let Ok(n) = key.parse::<i64>() {
                        if n.to_string() == key {
                            result.insert(next_int.to_string(), value);
                            next_int += 1;
                            continue;
                        }
                    }
                    result.insert(key, value);
                }
            }
            _ => panic!("array_merge(): Argument must be of type array"),
        }
    }
    let is_list = result.keys().enumerate().all(|(i, k)| *k == i.to_string());
    if is_list {
        PhpMixed::List(result.into_values().collect())
    } else {
        PhpMixed::Array(result)
    }
}

/// PHP `array_merge` for a string-keyed map that MAY also contain integer-like
/// keys. Typed counterpart of [`array_merge`] for `IndexMap<String, V>` values
/// (e.g. `Link` maps from `getProvides`/`getReplaces`).
///
/// Must reproduce the same mixed-key semantics as [`array_merge`]: string keys
/// overwrite in place (later wins), integer-like keys ("0","1",...) are appended
/// and renumbered sequentially across both inputs. A naive `IndexMap::insert`
/// per entry is INCORRECT because it would collide on shared integer keys.
pub fn array_merge_map<V>(
    array1: IndexMap<String, V>,
    array2: IndexMap<String, V>,
) -> IndexMap<String, V> {
    let mut result: IndexMap<String, V> = IndexMap::new();
    let mut next_int: i64 = 0;
    for array in [array1, array2] {
        for (key, value) in array {
            if let Ok(n) = key.parse::<i64>() {
                if n.to_string() == key {
                    result.insert(next_int.to_string(), value);
                    next_int += 1;
                    continue;
                }
            }
            result.insert(key, value);
        }
    }
    result
}

pub fn array_diff(_array1: &[String], _array2: &[String]) -> Vec<String> {
    _array1
        .iter()
        .filter(|&x| !_array2.contains(x))
        .cloned()
        .collect()
}

// PHP's default array_unique flag is SORT_STRING, comparing elements as
// strings. For the `String`/`&str`-like element types used by every caller,
// `PartialEq` is equivalent. First occurrence is kept, matching PHP.
pub fn array_unique<T: Clone + PartialEq>(array: &[T]) -> Vec<T> {
    let mut result: Vec<T> = Vec::new();
    for item in array {
        if !result.contains(item) {
            result.push(item.clone());
        }
    }
    result
}

pub fn array_intersect_key(
    _array1: &IndexMap<String, PhpMixed>,
    _array2: &IndexMap<String, PhpMixed>,
) -> IndexMap<String, PhpMixed> {
    _array1
        .iter()
        .filter(|(k, _)| _array2.contains_key(k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn array_replace_recursive(
    mut base: IndexMap<String, PhpMixed>,
    replacement: IndexMap<String, PhpMixed>,
) -> IndexMap<String, PhpMixed> {
    for (key, replacement_value) in replacement {
        let merged = match base.get(&key) {
            Some(base_value) => {
                array_replace_recursive_value(base_value.clone(), replacement_value)
            }
            None => replacement_value,
        };
        base.insert(key, merged);
    }
    base
}

// PHP recurses only when both the existing and the replacing value are arrays;
// otherwise the replacing value wins outright.
fn array_replace_recursive_value(base: PhpMixed, replacement: PhpMixed) -> PhpMixed {
    match (base, replacement) {
        (PhpMixed::Array(base), PhpMixed::Array(replacement)) => {
            PhpMixed::Array(array_replace_recursive_assoc(base, replacement))
        }
        (PhpMixed::List(base), PhpMixed::List(replacement)) => {
            PhpMixed::List(array_replace_recursive_list(base, replacement))
        }
        (_, replacement) => replacement,
    }
}

fn array_replace_recursive_assoc(
    mut base: IndexMap<String, PhpMixed>,
    replacement: IndexMap<String, PhpMixed>,
) -> IndexMap<String, PhpMixed> {
    for (key, replacement_value) in replacement {
        let merged = match base.get(&key) {
            Some(base_value) => {
                array_replace_recursive_value(base_value.clone(), replacement_value)
            }
            None => replacement_value,
        };
        base.insert(key, merged);
    }
    base
}

fn array_replace_recursive_list(
    mut base: Vec<PhpMixed>,
    replacement: Vec<PhpMixed>,
) -> Vec<PhpMixed> {
    for (index, replacement_value) in replacement.into_iter().enumerate() {
        if index < base.len() {
            base[index] = array_replace_recursive_value(base[index].clone(), replacement_value);
        } else {
            base.push(replacement_value);
        }
    }
    base
}

pub fn array_search_mixed(
    needle: &PhpMixed,
    haystack: &PhpMixed,
    strict: bool,
) -> Option<PhpMixed> {
    let matches = |value: &PhpMixed| -> bool {
        if strict {
            value == needle
        } else {
            loose_eq(value, needle)
        }
    };
    match haystack {
        PhpMixed::List(items) => items
            .iter()
            .position(matches)
            .map(|i| PhpMixed::Int(i as i64)),
        PhpMixed::Array(map) => map
            .iter()
            .find(|(_, value)| matches(value))
            .map(|(key, _)| php_key_to_mixed(key)),
        _ => None,
    }
}

pub fn array_search(needle: &str, haystack: &IndexMap<String, String>) -> Option<String> {
    haystack
        .iter()
        .find(|(_, value)| value.as_str() == needle)
        .map(|(key, _)| key.clone())
}

pub fn array_shift<T>(_array: &mut Vec<T>) -> Option<T> {
    if _array.is_empty() {
        None
    } else {
        Some(_array.remove(0))
    }
}

pub fn array_pop<T>(_array: &mut Vec<T>) -> Option<T> {
    _array.pop()
}

pub fn array_unshift<T>(_array: &mut Vec<T>, _value: T) {
    _array.insert(0, _value);
}

pub fn array_reverse<T: Clone>(_array: &[T], _preserve_keys: bool) -> Vec<T> {
    _array.iter().rev().cloned().collect()
}

pub fn array_filter<T: Clone, F>(_array: &[T], _callback: F) -> Vec<T>
where
    F: Fn(&T) -> bool,
{
    _array.iter().filter(|&x| _callback(x)).cloned().collect()
}

pub fn array_filter_map<F>(
    _array: &IndexMap<String, PhpMixed>,
    _callback: F,
) -> IndexMap<String, PhpMixed>
where
    F: Fn(&PhpMixed) -> bool,
{
    _array
        .iter()
        .filter(|&(_, v)| _callback(v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn array_all<T, F>(_array: &[T], _callback: F) -> bool
where
    F: Fn(&T) -> bool,
{
    _array.iter().all(_callback)
}

pub fn array_any<T, F>(_array: &[T], _callback: F) -> bool
where
    F: Fn(&T) -> bool,
{
    _array.iter().any(_callback)
}

pub fn array_reduce<T, U, F>(_array: &[T], _callback: F, _initial: U) -> U
where
    F: Fn(U, &T) -> U,
{
    _array.iter().fold(_initial, _callback)
}

pub fn array_intersect<T: Clone + PartialEq>(_array1: &[T], _array2: &[T]) -> Vec<T> {
    _array1
        .iter()
        .filter(|&x| _array2.contains(x))
        .cloned()
        .collect()
}

pub fn array_flip(array: &PhpMixed) -> PhpMixed {
    let mut result: IndexMap<String, PhpMixed> = IndexMap::new();
    match array {
        PhpMixed::List(items) => {
            for (i, value) in items.iter().enumerate() {
                match value {
                    PhpMixed::Int(n) => {
                        result.insert(n.to_string(), PhpMixed::Int(i as i64));
                    }
                    PhpMixed::String(s) => {
                        result.insert(s.clone(), PhpMixed::Int(i as i64));
                    }
                    // Non int/string values cannot be array keys and are skipped.
                    _ => {}
                }
            }
        }
        PhpMixed::Array(map) => {
            for (key, value) in map {
                match value {
                    PhpMixed::Int(n) => {
                        result.insert(n.to_string(), php_key_to_mixed(key));
                    }
                    PhpMixed::String(s) => {
                        result.insert(s.clone(), php_key_to_mixed(key));
                    }
                    _ => {}
                }
            }
        }
        _ => panic!("array_flip(): Argument #1 ($array) must be of type array"),
    }
    PhpMixed::Array(result)
}

pub fn array_flip_strings(_array: &[String]) -> IndexMap<String, PhpMixed> {
    _array
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), PhpMixed::Int(i as i64)))
        .collect()
}

pub fn array_key_exists<V>(_key: &str, _array: &IndexMap<String, V>) -> bool {
    _array.contains_key(_key)
}

pub fn array_is_list(array: &PhpMixed) -> bool {
    match array {
        PhpMixed::List(_) => true,
        PhpMixed::Array(map) => map.keys().enumerate().all(|(i, k)| *k == i.to_string()),
        _ => panic!("array_is_list(): Argument #1 ($array) must be of type array"),
    }
}

pub fn array_splice<T>(
    array: &mut Vec<T>,
    offset: i64,
    length: Option<i64>,
    replacement: Vec<T>,
) -> Vec<T> {
    let (start, end) = php_slice_bounds(array.len() as i64, offset, length);
    array.splice(start..end, replacement).collect()
}

pub fn array_pop_first<T>(array: &mut Vec<T>) -> Option<T> {
    if array.is_empty() {
        None
    } else {
        Some(array.remove(0))
    }
}

pub fn array_merge_recursive(arrays: Vec<PhpMixed>) -> PhpMixed {
    let mut acc: Vec<(MergeKey, PhpMixed)> = Vec::new();
    let mut next_int: i64 = 0;
    for arr in arrays {
        merge_recursive_into(&mut acc, &mut next_int, arr);
    }
    merge_build(acc)
}

#[derive(Clone)]
enum MergeKey {
    Int(i64),
    Str(String),
}

// Split a PHP array value into (key, value) entries. Integer and integer-like
// string keys become `Int`, everything else stays `Str`, matching how PHP
// normalises array keys.
fn merge_entries(value: PhpMixed) -> Vec<(MergeKey, PhpMixed)> {
    match value {
        PhpMixed::List(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (MergeKey::Int(i as i64), v))
            .collect(),
        PhpMixed::Array(map) => map
            .into_iter()
            .map(|(k, v)| (merge_parse_key(k), v))
            .collect(),
        _ => panic!("array_merge_recursive(): Argument must be of type array"),
    }
}

fn merge_parse_key(key: String) -> MergeKey {
    if let Ok(n) = key.parse::<i64>() {
        if n.to_string() == key {
            return MergeKey::Int(n);
        }
    }
    MergeKey::Str(key)
}

// Wrap a scalar into a single-element list, mirroring PHP's behaviour of
// promoting a non-array value to an array before a recursive merge.
fn merge_wrap_array(value: PhpMixed) -> PhpMixed {
    match value {
        PhpMixed::List(_) | PhpMixed::Array(_) => value,
        scalar => PhpMixed::List(vec![scalar]),
    }
}

fn merge_recursive_into(acc: &mut Vec<(MergeKey, PhpMixed)>, next_int: &mut i64, src: PhpMixed) {
    for (key, value) in merge_entries(src) {
        match key {
            MergeKey::Int(_) => {
                acc.push((MergeKey::Int(*next_int), value));
                *next_int += 1;
            }
            MergeKey::Str(s) => {
                let existing = acc
                    .iter()
                    .position(|(k, _)| matches!(k, MergeKey::Str(es) if *es == s));
                match existing {
                    Some(pos) => {
                        let merged = merge_two_recursive(acc[pos].1.clone(), value);
                        acc[pos].1 = merged;
                    }
                    None => acc.push((MergeKey::Str(s), value)),
                }
            }
        }
    }
}

fn merge_two_recursive(a: PhpMixed, b: PhpMixed) -> PhpMixed {
    let mut acc = merge_entries(merge_wrap_array(a));
    let mut next_int = acc
        .iter()
        .filter_map(|(k, _)| match k {
            MergeKey::Int(n) => Some(*n + 1),
            MergeKey::Str(_) => None,
        })
        .max()
        .unwrap_or(0);
    merge_recursive_into(&mut acc, &mut next_int, merge_wrap_array(b));
    merge_build(acc)
}

// Re-assemble entries into a PhpMixed, preferring a `List` when the keys are a
// dense 0..n integer sequence (PHP renders such an array as a list).
fn merge_build(acc: Vec<(MergeKey, PhpMixed)>) -> PhpMixed {
    let is_list = acc
        .iter()
        .enumerate()
        .all(|(i, (k, _))| matches!(k, MergeKey::Int(n) if *n == i as i64));
    if is_list {
        PhpMixed::List(acc.into_iter().map(|(_, v)| v).collect())
    } else {
        PhpMixed::Array(
            acc.into_iter()
                .map(|(k, v)| match k {
                    MergeKey::Int(n) => (n.to_string(), v),
                    MergeKey::Str(s) => (s, v),
                })
                .collect(),
        )
    }
}

pub fn array_slice<V: Clone>(
    array: &IndexMap<String, V>,
    offset: i64,
    length: Option<i64>,
) -> IndexMap<String, V> {
    let (start, end) = php_slice_bounds(array.len() as i64, offset, length);
    array
        .iter()
        .skip(start)
        .take(end - start)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn array_map<T, U, F>(_callback: F, _array: &[T]) -> Vec<U>
where
    F: Fn(&T) -> U,
{
    _array.iter().map(_callback).collect()
}

pub fn array_filter_use_key(
    _array: &IndexMap<String, PhpMixed>,
    _callback: Box<dyn Fn(&str) -> bool>,
) -> IndexMap<String, PhpMixed> {
    _array
        .iter()
        .filter(|(k, _)| _callback(k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn array_chunk<T: Clone>(_array: &[T], _size: i64, _preserve_keys: bool) -> Vec<Vec<T>> {
    _array.chunks(_size as usize).map(|c| c.to_vec()).collect()
}

pub fn array_diff_key(
    _array1: IndexMap<String, PhpMixed>,
    _array2: &IndexMap<String, PhpMixed>,
) -> IndexMap<String, PhpMixed> {
    _array1
        .into_iter()
        .filter(|(k, _)| !_array2.contains_key(k.as_str()))
        .collect()
}

/// Map a PHP array key (always stored as a `String` here) back to its PHP value
/// type: an integer-like key becomes an int, anything else stays a string.
fn php_key_to_mixed(key: &str) -> PhpMixed {
    if let Ok(n) = key.parse::<i64>() {
        if n.to_string() == key {
            return PhpMixed::Int(n);
        }
    }
    PhpMixed::String(key.to_string())
}

/// Resolve PHP array_slice/substr-style (offset, length) into a `[start, end)`
/// pair of indices, honouring negative offsets and lengths.
fn php_slice_bounds(len: i64, offset: i64, length: Option<i64>) -> (usize, usize) {
    let start = if offset < 0 {
        (len + offset).max(0)
    } else {
        offset.min(len)
    };
    let end = match length {
        None => len,
        Some(l) if l < 0 => (len + l).max(start),
        Some(l) => (start + l).min(len),
    };
    (start as usize, end as usize)
}

pub fn in_array(needle: PhpMixed, haystack: &PhpMixed, strict: bool) -> bool {
    let values: Vec<&PhpMixed> = match haystack {
        PhpMixed::List(items) => items.iter().collect(),
        PhpMixed::Array(map) => map.values().collect(),
        _ => return false,
    };

    if strict {
        values.iter().any(|value| **value == needle)
    } else {
        values.iter().any(|value| loose_eq(value, &needle))
    }
}

/// PHP numeric-string-aware conversion to a number, used by loose comparison.
fn loose_to_number(s: &str) -> Option<f64> {
    if crate::var::is_numeric_string(s) {
        s.trim().parse::<f64>().ok()
    } else {
        None
    }
}

fn loose_num_to_string(value: &PhpMixed) -> String {
    match value {
        PhpMixed::Int(i) => i.to_string(),
        PhpMixed::Float(f) => f.to_string(),
        _ => String::new(),
    }
}

fn loose_num_value(value: &PhpMixed) -> f64 {
    match value {
        PhpMixed::Int(i) => *i as f64,
        PhpMixed::Float(f) => *f,
        _ => 0.0,
    }
}

/// PHP `==` loose comparison.
pub fn loose_eq(a: &PhpMixed, b: &PhpMixed) -> bool {
    use PhpMixed::*;
    match (a, b) {
        (Null, Null) => true,
        // null compares loosely against the "empty"/false-y value of the other type.
        (Null, other) | (other, Null) => !crate::var::to_bool(other),
        // If either operand is a bool, both are converted to bool.
        (Bool(x), other) => *x == crate::var::to_bool(other),
        (other, Bool(y)) => crate::var::to_bool(other) == *y,
        (Int(x), Int(y)) => x == y,
        (Int(_) | Float(_), Int(_) | Float(_)) => loose_num_value(a) == loose_num_value(b),
        (String(x), String(y)) => match (loose_to_number(x), loose_to_number(y)) {
            (Some(nx), Some(ny)) => nx == ny,
            _ => x == y,
        },
        (Int(_) | Float(_), String(s)) => match loose_to_number(s) {
            Some(ns) => loose_num_value(a) == ns,
            None => loose_num_to_string(a) == *s,
        },
        (String(s), Int(_) | Float(_)) => match loose_to_number(s) {
            Some(ns) => loose_num_value(b) == ns,
            None => *s == loose_num_to_string(b),
        },
        (List(x), List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| loose_eq(p, q))
        }
        (Array(x), Array(y)) | (Object(x), Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).map(|w| loose_eq(v, w)).unwrap_or(false))
        }
        _ => false,
    }
}

pub fn krsort<V>(array: &mut IndexMap<i64, V>) {
    array.sort_by(|k1, _, k2, _| k2.cmp(k1));
}

pub fn uasort<T, F>(array: &mut Vec<T>, compare: F)
where
    F: FnMut(&T, &T) -> i64,
{
    let mut compare = compare;
    array.sort_by(|a, b| compare(a, b).cmp(&0));
}

pub fn uasort_map<K, V, F>(array: &mut IndexMap<K, V>, compare: F)
where
    F: FnMut(&V, &V) -> i64,
{
    let mut compare = compare;
    array.sort_by(|_, v1, _, v2| compare(v1, v2).cmp(&0));
}

pub fn sort<T: Ord>(_array: &mut Vec<T>) {
    _array.sort();
}

pub fn sort_with_flags<T: Ord>(array: &mut Vec<T>, flags: i64) {
    if flags != SORT_REGULAR {
        // TODO(phase-d): flag-specific comparison (SORT_NUMERIC/SORT_STRING/
        // SORT_NATURAL/SORT_FLAG_CASE) cannot be expressed for a generic
        // `T: Ord` element. No caller passes a non-regular flag yet.
        todo!("sort() with flags other than SORT_REGULAR");
    }
    array.sort();
}

pub const SORT_REGULAR: i64 = 0;
pub const SORT_NUMERIC: i64 = 1;
pub const SORT_STRING: i64 = 2;
pub const SORT_NATURAL: i64 = 6;
pub const SORT_FLAG_CASE: i64 = 8;

pub fn usort<T, F>(_array: &mut Vec<T>, _compare: F)
where
    F: FnMut(&T, &T) -> i64,
{
    let mut compare = _compare;
    _array.sort_by(|a, b| compare(a, b).cmp(&0));
}

pub fn ksort<V>(array: &mut IndexMap<String, V>) {
    array.sort_by(|k1, _, k2, _| php_sort_regular_key(k1, k2));
}

// PHP's default SORT_REGULAR comparison for array keys: two integer-like keys
// compare numerically, otherwise byte-wise as strings.
// TODO(phase-d): full SORT_REGULAR semantics for mixed integer/non-numeric-string
// keys are not reproduced; every current caller uses homogeneous string keys.
fn php_sort_regular_key(a: &str, b: &str) -> std::cmp::Ordering {
    if let (Ok(na), Ok(nb)) = (a.parse::<i64>(), b.parse::<i64>()) {
        if na.to_string() == a && nb.to_string() == b {
            return na.cmp(&nb);
        }
    }
    a.cmp(b)
}

pub fn asort<V: Ord>(array: &mut IndexMap<String, V>) {
    array.sort_by(|_, v1, _, v2| v1.cmp(v2));
}

pub fn uksort<V, F>(array: &mut IndexMap<String, V>, callback: F)
where
    F: FnMut(&str, &str) -> i64,
{
    let mut callback = callback;
    array.sort_by(|k1, _, k2, _| callback(k1, k2).cmp(&0));
}

pub fn sort_natural_flag_case(values: &mut Vec<String>) {
    values.sort_by(|a, b| crate::strnatcasecmp(a, b).cmp(&0));
}

pub fn count_mixed(value: &PhpMixed) -> i64 {
    count(value) as i64
}

pub fn count(value: &PhpMixed) -> usize {
    match value {
        PhpMixed::List(items) => items.len(),
        PhpMixed::Array(entries) => entries.len(),
        PhpMixed::Object(object) => object.len(),
        // PHP 8 throws a `TypeError` for non-countable arguments.
        PhpMixed::Null
        | PhpMixed::Bool(_)
        | PhpMixed::Int(_)
        | PhpMixed::Float(_)
        | PhpMixed::String(_) => {
            panic!("count(): Argument #1 ($value) must be of type Countable|array")
        }
    }
}

pub fn iterator_to_array<I>(iter: I) -> Vec<I::Item>
where
    I: IntoIterator,
{
    iter.into_iter().collect()
}
