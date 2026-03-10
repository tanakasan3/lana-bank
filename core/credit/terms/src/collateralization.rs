use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Collateralization ratio. Serializes as a nullable decimal:
/// - `Finite(x)` → the decimal value
/// - `Infinite` → `null`
///
/// Deserialization is backwards-compatible with the legacy tagged enum format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollateralizationRatio {
    Finite(Decimal),
    Infinite,
}

impl Default for CollateralizationRatio {
    fn default() -> Self {
        Self::Finite(Decimal::ZERO)
    }
}

impl Serialize for CollateralizationRatio {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            CollateralizationRatio::Finite(d) => serializer.serialize_some(d),
            CollateralizationRatio::Infinite => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for CollateralizationRatio {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de;

        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = CollateralizationRatio;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a decimal number, null, string \"Infinite\", or object {\"Finite\": ...}",
                )
            }

            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(CollateralizationRatio::Infinite)
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(CollateralizationRatio::Infinite)
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(CollateralizationRatio::Finite(Decimal::from(v)))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(CollateralizationRatio::Finite(Decimal::from(v)))
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Decimal::try_from(v)
                    .map(CollateralizationRatio::Finite)
                    .map_err(de::Error::custom)
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                if v == "Infinite" {
                    Ok(CollateralizationRatio::Infinite)
                } else {
                    v.parse::<Decimal>()
                        .map(CollateralizationRatio::Finite)
                        .map_err(de::Error::custom)
                }
            }

            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| de::Error::custom("expected key in map"))?;
                if key == "Finite" {
                    let value: Decimal = map.next_value()?;
                    Ok(CollateralizationRatio::Finite(value))
                } else {
                    Err(de::Error::unknown_field(&key, &["Finite"]))
                }
            }

            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                deserializer.deserialize_any(Visitor)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(feature = "json-schema")]
impl schemars::JsonSchema for CollateralizationRatio {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CollateralizationRatio".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let decimal_schema = generator.subschema_for::<Decimal>();
        let null_schema = schemars::json_schema!({ "type": "null" });

        schemars::json_schema!({
            "description": "Collateralization ratio. null represents Infinite.",
            "anyOf": [decimal_schema, null_schema]
        })
    }
}

#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    Eq,
    strum::Display,
    strum::EnumString,
)]
#[cfg_attr(feature = "graphql", derive(async_graphql::Enum))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
pub enum CollateralizationState {
    FullyCollateralized,
    UnderMarginCallThreshold,
    UnderLiquidationThreshold,
    #[default]
    NoCollateral,
    NoExposure,
}

impl CollateralizationState {
    pub const fn is_under_liquidation_threshold(&self) -> bool {
        matches!(self, Self::UnderLiquidationThreshold)
    }
}

#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    Eq,
    strum::Display,
    strum::EnumString,
)]
#[cfg_attr(feature = "graphql", derive(async_graphql::Enum))]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
pub enum PendingCreditFacilityCollateralizationState {
    FullyCollateralized,
    UnderCollateralized,
    #[default]
    NotYetCollateralized,
}

// SQLx implementations for database storage
mod collateralization_state_sqlx {
    use sqlx::{Type, postgres::*};

    use super::CollateralizationState;

    impl Type<Postgres> for CollateralizationState {
        fn type_info() -> PgTypeInfo {
            <String as Type<Postgres>>::type_info()
        }

        fn compatible(ty: &PgTypeInfo) -> bool {
            <String as Type<Postgres>>::compatible(ty)
        }
    }

    impl sqlx::Encode<'_, Postgres> for CollateralizationState {
        fn encode_by_ref(
            &self,
            buf: &mut PgArgumentBuffer,
        ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Sync + Send>> {
            <String as sqlx::Encode<'_, Postgres>>::encode(self.to_string(), buf)
        }
    }

    impl<'r> sqlx::Decode<'r, Postgres> for CollateralizationState {
        fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            let s = <String as sqlx::Decode<Postgres>>::decode(value)?;
            Ok(s.parse().map_err(|e: strum::ParseError| Box::new(e))?)
        }
    }

    impl PgHasArrayType for CollateralizationState {
        fn array_type_info() -> PgTypeInfo {
            <String as sqlx::postgres::PgHasArrayType>::array_type_info()
        }
    }
}

mod collateralization_ratio_sqlx {
    use rust_decimal::Decimal;
    use sqlx::{Type, postgres::*};

    use super::CollateralizationRatio;

    impl Type<Postgres> for CollateralizationRatio {
        fn type_info() -> PgTypeInfo {
            <Option<Decimal> as Type<Postgres>>::type_info()
        }

        fn compatible(ty: &PgTypeInfo) -> bool {
            <Option<Decimal> as Type<Postgres>>::compatible(ty)
        }
    }

    impl sqlx::Encode<'_, Postgres> for CollateralizationRatio {
        fn encode_by_ref(
            &self,
            buf: &mut PgArgumentBuffer,
        ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Sync + Send>> {
            let opt: Option<Decimal> = match *self {
                CollateralizationRatio::Finite(d) => Some(d),
                CollateralizationRatio::Infinite => None,
            };
            <Option<Decimal> as sqlx::Encode<'_, Postgres>>::encode(opt, buf)
        }
    }

    impl<'r> sqlx::Decode<'r, Postgres> for CollateralizationRatio {
        fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            let opt: Option<Decimal> = <Option<Decimal> as sqlx::Decode<Postgres>>::decode(value)?;
            Ok(match opt {
                Some(d) => CollateralizationRatio::Finite(d),
                None => CollateralizationRatio::Infinite,
            })
        }
    }

    impl PgHasArrayType for CollateralizationRatio {
        fn array_type_info() -> PgTypeInfo {
            <Option<Decimal> as sqlx::postgres::PgHasArrayType>::array_type_info()
        }
    }
}

mod pending_collateralization_state_sqlx {
    use sqlx::{Type, postgres::*};

    use super::PendingCreditFacilityCollateralizationState;

    impl Type<Postgres> for PendingCreditFacilityCollateralizationState {
        fn type_info() -> PgTypeInfo {
            <String as Type<Postgres>>::type_info()
        }

        fn compatible(ty: &PgTypeInfo) -> bool {
            <String as Type<Postgres>>::compatible(ty)
        }
    }

    impl sqlx::Encode<'_, Postgres> for PendingCreditFacilityCollateralizationState {
        fn encode_by_ref(
            &self,
            buf: &mut PgArgumentBuffer,
        ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Sync + Send>> {
            <String as sqlx::Encode<'_, Postgres>>::encode(self.to_string(), buf)
        }
    }

    impl<'r> sqlx::Decode<'r, Postgres> for PendingCreditFacilityCollateralizationState {
        fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            let s = <String as sqlx::Decode<Postgres>>::decode(value)?;
            Ok(s.parse().map_err(|e: strum::ParseError| Box::new(e))?)
        }
    }

    impl PgHasArrayType for PendingCreditFacilityCollateralizationState {
        fn array_type_info() -> PgTypeInfo {
            <String as sqlx::postgres::PgHasArrayType>::array_type_info()
        }
    }
}

#[cfg(test)]
mod test {
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    use super::*;

    #[test]
    fn finite_serializes_as_decimal() {
        let ratio = CollateralizationRatio::Finite(dec!(125));
        let json = serde_json::to_string(&ratio).unwrap();
        assert_eq!(json, "\"125\"");
    }

    #[test]
    fn infinite_serializes_as_null() {
        let ratio = CollateralizationRatio::Infinite;
        let json = serde_json::to_string(&ratio).unwrap();
        assert_eq!(json, "null");
    }

    #[test]
    fn deserializes_new_format() {
        let ratio: CollateralizationRatio = serde_json::from_str("\"125.50\"").unwrap();
        assert_eq!(ratio, CollateralizationRatio::Finite(dec!(125.50)));

        let ratio: CollateralizationRatio = serde_json::from_str("null").unwrap();
        assert_eq!(ratio, CollateralizationRatio::Infinite);
    }

    #[test]
    fn deserializes_legacy_format() {
        let ratio: CollateralizationRatio =
            serde_json::from_str(r#"{"Finite":"125"}"#).unwrap();
        assert_eq!(ratio, CollateralizationRatio::Finite(dec!(125)));

        let ratio: CollateralizationRatio = serde_json::from_str(r#""Infinite""#).unwrap();
        assert_eq!(ratio, CollateralizationRatio::Infinite);
    }

    #[test]
    fn roundtrip() {
        let original = CollateralizationRatio::Finite(dec!(140));
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: CollateralizationRatio = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);

        let original = CollateralizationRatio::Infinite;
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: CollateralizationRatio = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn default_is_finite_zero() {
        assert_eq!(
            CollateralizationRatio::default(),
            CollateralizationRatio::Finite(Decimal::ZERO)
        );
    }
}
