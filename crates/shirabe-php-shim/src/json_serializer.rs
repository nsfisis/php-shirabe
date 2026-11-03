#![expect(
    clippy::disallowed_types,
    reason = "serde's Serializer trait takes names and JSON strings as text"
)]

use crate::utf8_text;
use serde::ser::{
    Error as _, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

/// A value serialized as JSON the way PHP's `json_encode()` writes it. The `Serialize` impls of
/// this crate pass PHP strings to `serialize_bytes`, which a JSON serializer would write as an
/// array of numbers; through this wrapper they are written as JSON strings instead. A string that
/// is not valid UTF-8 fails with `JSON_ERROR_UTF8`, or loses the invalid bytes under
/// `JSON_INVALID_UTF8_IGNORE`.
#[derive(Debug)]
pub struct PhpJson<'a, T: ?Sized> {
    value: &'a T,
    invalid_utf8_ignore: bool,
}

impl<'a, T: Serialize + ?Sized> PhpJson<'a, T> {
    pub fn new(value: &'a T) -> Self {
        Self {
            value,
            invalid_utf8_ignore: false,
        }
    }

    pub fn with_invalid_utf8_ignore(value: &'a T, invalid_utf8_ignore: bool) -> Self {
        Self {
            value,
            invalid_utf8_ignore,
        }
    }
}

impl<T: Serialize + ?Sized> Serialize for PhpJson<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(PhpJsonSerializer {
            inner: serializer,
            invalid_utf8_ignore: self.invalid_utf8_ignore,
        })
    }
}

#[derive(Debug)]
struct PhpJsonSerializer<S> {
    inner: S,
    invalid_utf8_ignore: bool,
}

#[derive(Debug)]
struct Compound<C> {
    inner: C,
    invalid_utf8_ignore: bool,
}

impl<S> PhpJsonSerializer<S> {
    fn wrap<'a, T: ?Sized>(&self, value: &'a T) -> PhpJson<'a, T> {
        PhpJson {
            value,
            invalid_utf8_ignore: self.invalid_utf8_ignore,
        }
    }
}

impl<C> Compound<C> {
    fn wrap<'a, T: ?Sized>(&self, value: &'a T) -> PhpJson<'a, T> {
        PhpJson {
            value,
            invalid_utf8_ignore: self.invalid_utf8_ignore,
        }
    }
}

impl<S: Serializer> Serializer for PhpJsonSerializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Compound<S::SerializeSeq>;
    type SerializeTuple = Compound<S::SerializeTuple>;
    type SerializeTupleStruct = Compound<S::SerializeTupleStruct>;
    type SerializeTupleVariant = Compound<S::SerializeTupleVariant>;
    type SerializeMap = Compound<S::SerializeMap>;
    type SerializeStruct = Compound<S::SerializeStruct>;
    type SerializeStructVariant = Compound<S::SerializeStructVariant>;

    fn serialize_bool(self, v: bool) -> Result<S::Ok, S::Error> {
        self.inner.serialize_bool(v)
    }

    fn serialize_i8(self, v: i8) -> Result<S::Ok, S::Error> {
        self.inner.serialize_i8(v)
    }

    fn serialize_i16(self, v: i16) -> Result<S::Ok, S::Error> {
        self.inner.serialize_i16(v)
    }

    fn serialize_i32(self, v: i32) -> Result<S::Ok, S::Error> {
        self.inner.serialize_i32(v)
    }

    fn serialize_i64(self, v: i64) -> Result<S::Ok, S::Error> {
        self.inner.serialize_i64(v)
    }

    fn serialize_i128(self, v: i128) -> Result<S::Ok, S::Error> {
        self.inner.serialize_i128(v)
    }

    fn serialize_u8(self, v: u8) -> Result<S::Ok, S::Error> {
        self.inner.serialize_u8(v)
    }

    fn serialize_u16(self, v: u16) -> Result<S::Ok, S::Error> {
        self.inner.serialize_u16(v)
    }

    fn serialize_u32(self, v: u32) -> Result<S::Ok, S::Error> {
        self.inner.serialize_u32(v)
    }

    fn serialize_u64(self, v: u64) -> Result<S::Ok, S::Error> {
        self.inner.serialize_u64(v)
    }

    fn serialize_u128(self, v: u128) -> Result<S::Ok, S::Error> {
        self.inner.serialize_u128(v)
    }

    fn serialize_f32(self, v: f32) -> Result<S::Ok, S::Error> {
        self.inner.serialize_f32(v)
    }

    fn serialize_f64(self, v: f64) -> Result<S::Ok, S::Error> {
        self.inner.serialize_f64(v)
    }

    fn serialize_char(self, v: char) -> Result<S::Ok, S::Error> {
        self.inner.serialize_char(v)
    }

    fn serialize_str(self, v: &str) -> Result<S::Ok, S::Error> {
        self.inner.serialize_str(v)
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<S::Ok, S::Error> {
        if let Some(text) = utf8_text(v) {
            return self.inner.serialize_str(text);
        }
        if !self.invalid_utf8_ignore {
            return Err(S::Error::custom(
                "Malformed UTF-8 characters, possibly incorrectly encoded",
            ));
        }
        let text: String = v.utf8_chunks().map(|chunk| chunk.valid()).collect();
        self.inner.serialize_str(&text)
    }

    fn serialize_none(self) -> Result<S::Ok, S::Error> {
        self.inner.serialize_none()
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<S::Ok, S::Error> {
        let value = self.wrap(value);
        self.inner.serialize_some(&value)
    }

    fn serialize_unit(self) -> Result<S::Ok, S::Error> {
        self.inner.serialize_unit()
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<S::Ok, S::Error> {
        self.inner.serialize_unit_struct(name)
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<S::Ok, S::Error> {
        self.inner
            .serialize_unit_variant(name, variant_index, variant)
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<S::Ok, S::Error> {
        let value = self.wrap(value);
        self.inner.serialize_newtype_struct(name, &value)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<S::Ok, S::Error> {
        let value = self.wrap(value);
        self.inner
            .serialize_newtype_variant(name, variant_index, variant, &value)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, S::Error> {
        let invalid_utf8_ignore = self.invalid_utf8_ignore;
        Ok(Compound {
            inner: self.inner.serialize_seq(len)?,
            invalid_utf8_ignore,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, S::Error> {
        let invalid_utf8_ignore = self.invalid_utf8_ignore;
        Ok(Compound {
            inner: self.inner.serialize_tuple(len)?,
            invalid_utf8_ignore,
        })
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, S::Error> {
        let invalid_utf8_ignore = self.invalid_utf8_ignore;
        Ok(Compound {
            inner: self.inner.serialize_tuple_struct(name, len)?,
            invalid_utf8_ignore,
        })
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, S::Error> {
        let invalid_utf8_ignore = self.invalid_utf8_ignore;
        Ok(Compound {
            inner: self
                .inner
                .serialize_tuple_variant(name, variant_index, variant, len)?,
            invalid_utf8_ignore,
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, S::Error> {
        let invalid_utf8_ignore = self.invalid_utf8_ignore;
        Ok(Compound {
            inner: self.inner.serialize_map(len)?,
            invalid_utf8_ignore,
        })
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, S::Error> {
        let invalid_utf8_ignore = self.invalid_utf8_ignore;
        Ok(Compound {
            inner: self.inner.serialize_struct(name, len)?,
            invalid_utf8_ignore,
        })
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, S::Error> {
        let invalid_utf8_ignore = self.invalid_utf8_ignore;
        Ok(Compound {
            inner: self
                .inner
                .serialize_struct_variant(name, variant_index, variant, len)?,
            invalid_utf8_ignore,
        })
    }

    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

impl<C: SerializeSeq> SerializeSeq for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), C::Error> {
        let value = self.wrap(value);
        self.inner.serialize_element(&value)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.inner.end()
    }
}

impl<C: SerializeTuple> SerializeTuple for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), C::Error> {
        let value = self.wrap(value);
        self.inner.serialize_element(&value)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.inner.end()
    }
}

impl<C: SerializeTupleStruct> SerializeTupleStruct for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), C::Error> {
        let value = self.wrap(value);
        self.inner.serialize_field(&value)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.inner.end()
    }
}

impl<C: SerializeTupleVariant> SerializeTupleVariant for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), C::Error> {
        let value = self.wrap(value);
        self.inner.serialize_field(&value)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.inner.end()
    }
}

impl<C: SerializeMap> SerializeMap for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), C::Error> {
        let key = self.wrap(key);
        self.inner.serialize_key(&key)
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), C::Error> {
        let value = self.wrap(value);
        self.inner.serialize_value(&value)
    }

    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), C::Error> {
        let key = self.wrap(key);
        let value = self.wrap(value);
        self.inner.serialize_entry(&key, &value)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.inner.end()
    }
}

impl<C: SerializeStruct> SerializeStruct for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), C::Error> {
        let value = self.wrap(value);
        self.inner.serialize_field(key, &value)
    }

    fn skip_field(&mut self, key: &'static str) -> Result<(), C::Error> {
        self.inner.skip_field(key)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.inner.end()
    }
}

impl<C: SerializeStructVariant> SerializeStructVariant for Compound<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), C::Error> {
        let value = self.wrap(value);
        self.inner.serialize_field(key, &value)
    }

    fn skip_field(&mut self, key: &'static str) -> Result<(), C::Error> {
        self.inner.skip_field(key)
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.inner.end()
    }
}
