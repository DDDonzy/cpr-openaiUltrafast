//! 仅替换顶层档位的原始字节，保留其余 JSON 数值、转义和字段次序

use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;

#[derive(Debug, thiserror::Error)]
#[error("ambiguous or invalid request JSON")]
pub struct RewriteError;

struct ObjectFields<'a> {
    count: usize,
    tier: Option<&'a RawValue>,
}

impl<'de> Deserialize<'de> for ObjectFields<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value = ObjectFields<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object with at most one service_tier")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut fields = ObjectFields {
                    count: 0,
                    tier: None,
                };
                while let Some(key) = map.next_key::<String>()? {
                    let value = map.next_value::<&'de RawValue>()?;
                    fields.count += 1;
                    if key == "service_tier" && fields.tier.replace(value).is_some() {
                        return Err(de::Error::custom("duplicate service tier"));
                    }
                }
                Ok(fields)
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}

pub fn ultrafast(body: &[u8]) -> Result<Option<Vec<u8>>, RewriteError> {
    let fields: ObjectFields<'_> = serde_json::from_slice(body).map_err(|_| RewriteError)?;
    let (start, end, replacement): (usize, usize, &[u8]) = if let Some(value) = fields.tier {
        if serde_json::from_str::<String>(value.get()).ok().as_deref() == Some("ultrafast") {
            return Ok(None);
        }
        let start = (value.get().as_ptr() as usize)
            .checked_sub(body.as_ptr() as usize)
            .ok_or(RewriteError)?;
        let end = start.checked_add(value.get().len()).ok_or(RewriteError)?;
        if end > body.len() {
            return Err(RewriteError);
        }
        (start, end, br#""ultrafast""#)
    } else {
        let end = body
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .ok_or(RewriteError)?;
        if body[end] != b'}' {
            return Err(RewriteError);
        }
        let replacement = if fields.count == 0 {
            &br#""service_tier":"ultrafast""#[..]
        } else {
            &br#", "service_tier":"ultrafast""#[..]
        };
        (end, end, replacement)
    };
    let mut rewritten = Vec::with_capacity(body.len() - (end - start) + replacement.len());
    rewritten.extend_from_slice(&body[..start]);
    rewritten.extend_from_slice(replacement);
    rewritten.extend_from_slice(&body[end..]);
    Ok(Some(rewritten))
}
