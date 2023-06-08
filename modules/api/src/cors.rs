use anyhow::bail;
use serde::{de, ser};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AllowOrigin {
    Any,
    Whitelist(Vec<String>),
}

impl ser::Serialize for AllowOrigin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match *self {
            AllowOrigin::Any => "*".serialize(serializer),
            AllowOrigin::Whitelist(ref hosts) => {
                if hosts.len() == 1 {
                    hosts[0].serialize(serializer)
                } else {
                    hosts.serialize(serializer)
                }
            }
        }
    }
}

impl<'de> de::Deserialize<'de> for AllowOrigin {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = AllowOrigin;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a list of hosts or \"*\"")
            }

            fn visit_str<E>(self, value: &str) -> Result<AllowOrigin, E>
            where
                E: de::Error,
            {
                match value {
                    "*" => Ok(AllowOrigin::Any),
                    _ => Ok(AllowOrigin::Whitelist(vec![value.to_string()])),
                }
            }

            fn visit_seq<A>(self, seq: A) -> Result<AllowOrigin, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let hosts =
                    de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(AllowOrigin::Whitelist(hosts))
            }
        }

        d.deserialize_any(Visitor)
    }
}

impl FromStr for AllowOrigin {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "*" {
            return Ok(AllowOrigin::Any);
        }

        let v: Vec<_> = s
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if v.is_empty() {
            bail!("Invalid AllowOrigin::Whitelist value");
        }

        Ok(AllowOrigin::Whitelist(v))
    }
}
