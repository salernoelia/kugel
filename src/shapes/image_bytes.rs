use base64::Engine;
use serde::de::{Deserializer, Error, SeqAccess, Visitor};
use serde::Serializer;
use std::fmt;

pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    s.serialize_str(&encoded)
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    struct BytesVisitor;

    impl<'de> Visitor<'de> for BytesVisitor {
        type Value = Vec<u8>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a base64 string or an array of bytes")
        }

        fn visit_str<E: Error>(self, v: &str) -> Result<Vec<u8>, E> {
            base64::engine::general_purpose::STANDARD
                .decode(v)
                .map_err(E::custom)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(b) = seq.next_element::<u8>()? {
                out.push(b);
            }
            Ok(out)
        }
    }

    d.deserialize_any(BytesVisitor)
}
