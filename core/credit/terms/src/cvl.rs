use rust_decimal::{Decimal, prelude::*};
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

use money::UsdCents;

use std::fmt;

/// Collateral-to-Value percentage. Serializes as a nullable decimal:
/// - `Finite(x)` → the decimal value (e.g. `"140.00"`)
/// - `Infinite` → `null`
///
/// Deserialization is backwards-compatible: accepts both the new flat format
/// and the legacy tagged enum format (`{"Finite": "140"}` / `"Infinite"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CVLPct {
    Finite(Decimal),
    Infinite,
}

impl Serialize for CVLPct {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            CVLPct::Finite(d) => serializer.serialize_some(d),
            CVLPct::Infinite => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for CVLPct {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de;

        struct CVLPctVisitor;

        impl<'de> de::Visitor<'de> for CVLPctVisitor {
            type Value = CVLPct;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a decimal number, null, string \"Infinite\", or object {\"Finite\": ...}",
                )
            }

            // New format: null → Infinite
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(CVLPct::Infinite)
            }

            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(CVLPct::Infinite)
            }

            // New format: number → Finite
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(CVLPct::Finite(Decimal::from(v)))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(CVLPct::Finite(Decimal::from(v)))
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Decimal::try_from(v)
                    .map(CVLPct::Finite)
                    .map_err(de::Error::custom)
            }

            // Could be legacy "Infinite" string or new decimal-as-string
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                if v == "Infinite" {
                    Ok(CVLPct::Infinite)
                } else {
                    v.parse::<Decimal>()
                        .map(CVLPct::Finite)
                        .map_err(de::Error::custom)
                }
            }

            // Legacy format: {"Finite": "140"} or new format wrapped in Some
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| de::Error::custom("expected key in map"))?;
                if key == "Finite" {
                    let value: Decimal = map.next_value()?;
                    Ok(CVLPct::Finite(value))
                } else {
                    Err(de::Error::unknown_field(&key, &["Finite"]))
                }
            }

            fn visit_some<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<Self::Value, D::Error> {
                deserializer.deserialize_any(CVLPctVisitor)
            }
        }

        deserializer.deserialize_any(CVLPctVisitor)
    }
}

#[cfg(feature = "json-schema")]
impl schemars::JsonSchema for CVLPct {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CVLPct".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let decimal_schema = generator.subschema_for::<Decimal>();
        let null_schema = schemars::json_schema!({ "type": "null" });

        schemars::json_schema!({
            "description": "Collateral-to-Value percentage. null represents Infinite.",
            "anyOf": [decimal_schema, null_schema]
        })
    }
}

#[cfg(feature = "graphql")]
async_graphql::scalar!(CVLPct);

impl PartialOrd for CVLPct {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering;
        match (self, other) {
            (Self::Infinite, Self::Infinite) => Some(Ordering::Equal),
            (Self::Infinite, Self::Finite(_)) => Some(Ordering::Greater),
            (Self::Finite(_), Self::Infinite) => Some(Ordering::Less),
            (Self::Finite(a), Self::Finite(b)) => a.partial_cmp(b),
        }
    }
}

impl CVLPct {
    pub const ZERO: Self = Self::Finite(dec!(0));
    pub const UPGRADE_BUFFER: Self = Self::Finite(dec!(5));

    pub fn new(value: u64) -> Self {
        Self::Finite(Decimal::from(value))
    }

    pub fn from_loan_amounts(
        collateral_value: UsdCents,
        total_outstanding_amount: UsdCents,
    ) -> Self {
        if collateral_value.is_zero() {
            return Self::ZERO;
        }

        if total_outstanding_amount.is_zero() {
            return Self::Infinite;
        }

        let ratio = (collateral_value.to_usd() / total_outstanding_amount.to_usd())
            .round_dp_with_strategy(2, RoundingStrategy::ToZero)
            * dec!(100);

        CVLPct::Finite(ratio)
    }

    pub fn is_zero(&self) -> bool {
        matches!(self, Self::Finite(value) if *value == dec!(0))
    }

    pub fn scale(&self, value: UsdCents) -> UsdCents {
        match self {
            Self::Finite(pct) => {
                let cents = value.to_usd() * dec!(100) * (pct / dec!(100));
                UsdCents::from(
                    cents
                        .round_dp_with_strategy(0, RoundingStrategy::AwayFromZero)
                        .to_u64()
                        .expect("should return a valid integer"),
                )
            }
            Self::Infinite => unreachable!("Cannot scale with infinite CVL percentage"),
        }
    }

    pub fn is_significantly_lower_than(&self, other: CVLPct, buffer: CVLPct) -> bool {
        other > *self + buffer
    }

    #[cfg(test)]
    pub fn target_value_given_outstanding(&self, outstanding: UsdCents) -> UsdCents {
        match self {
            Self::Finite(pct) => {
                let target_in_usd = pct / dec!(100) * outstanding.to_usd();
                UsdCents::from(
                    (target_in_usd * dec!(100))
                        .round_dp_with_strategy(0, RoundingStrategy::AwayFromZero)
                        .to_u64()
                        .expect("should return a valid integer"),
                )
            }
            Self::Infinite => {
                unreachable!("Cannot calculate target value for infinite CVL percentage")
            }
        }
    }
}

impl fmt::Display for CVLPct {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> fmt::Result {
        match self {
            Self::Finite(value) => write!(f, "{value}"),
            Self::Infinite => write!(f, "∞"),
        }
    }
}

impl std::ops::Add for CVLPct {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Finite(a), Self::Finite(b)) => Self::Finite(a + b),
            _ => Self::Infinite,
        }
    }
}

#[cfg(test)]
impl std::ops::Sub for CVLPct {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        match (self, other) {
            (Self::Infinite, Self::Finite(_)) => Self::Infinite,
            (Self::Finite(_), Self::Infinite) => panic!("Cannot subtract infinite from finite"),
            (Self::Infinite, Self::Infinite) => panic!("Infinite - Infinite is undefined"),
            (Self::Finite(a), Self::Finite(b)) => Self::Finite(a - b),
        }
    }
}

impl From<Decimal> for CVLPct {
    fn from(value: Decimal) -> Self {
        CVLPct::Finite(value)
    }
}

#[cfg(test)]
mod test {
    use rust_decimal_macros::dec;

    use super::*;

    mod serde_roundtrip {
        use super::*;

        #[test]
        fn finite_serializes_as_decimal() {
            let cvl = CVLPct::Finite(dec!(140));
            let json = serde_json::to_string(&cvl).unwrap();
            assert_eq!(json, "\"140\"");
        }

        #[test]
        fn infinite_serializes_as_null() {
            let cvl = CVLPct::Infinite;
            let json = serde_json::to_string(&cvl).unwrap();
            assert_eq!(json, "null");
        }

        #[test]
        fn deserializes_new_format_decimal() {
            let cvl: CVLPct = serde_json::from_str("\"140\"").unwrap();
            assert_eq!(cvl, CVLPct::Finite(dec!(140)));
        }

        #[test]
        fn deserializes_new_format_number() {
            let cvl: CVLPct = serde_json::from_str("140").unwrap();
            assert_eq!(cvl, CVLPct::Finite(dec!(140)));
        }

        #[test]
        fn deserializes_new_format_null() {
            let cvl: CVLPct = serde_json::from_str("null").unwrap();
            assert_eq!(cvl, CVLPct::Infinite);
        }

        #[test]
        fn deserializes_legacy_tagged_finite() {
            let cvl: CVLPct = serde_json::from_str(r#"{"Finite":"140"}"#).unwrap();
            assert_eq!(cvl, CVLPct::Finite(dec!(140)));
        }

        #[test]
        fn deserializes_legacy_tagged_infinite() {
            let cvl: CVLPct = serde_json::from_str(r#""Infinite""#).unwrap();
            assert_eq!(cvl, CVLPct::Infinite);
        }

        #[test]
        fn roundtrip_finite() {
            let original = CVLPct::Finite(dec!(125.50));
            let json = serde_json::to_string(&original).unwrap();
            let deserialized: CVLPct = serde_json::from_str(&json).unwrap();
            assert_eq!(original, deserialized);
        }

        #[test]
        fn roundtrip_infinite() {
            let original = CVLPct::Infinite;
            let json = serde_json::to_string(&original).unwrap();
            let deserialized: CVLPct = serde_json::from_str(&json).unwrap();
            assert_eq!(original, deserialized);
        }

        #[test]
        fn deserializes_in_struct_context() {
            // Simulates how it appears nested in TermValues JSON
            #[derive(serde::Deserialize, Debug, PartialEq)]
            struct Terms {
                liquidation_cvl: CVLPct,
                margin_call_cvl: CVLPct,
            }

            // New format
            let new_json = r#"{"liquidation_cvl":"105","margin_call_cvl":null}"#;
            let terms: Terms = serde_json::from_str(new_json).unwrap();
            assert_eq!(terms.liquidation_cvl, CVLPct::Finite(dec!(105)));
            assert_eq!(terms.margin_call_cvl, CVLPct::Infinite);

            // Legacy format
            let legacy_json =
                r#"{"liquidation_cvl":{"Finite":"105"},"margin_call_cvl":"Infinite"}"#;
            let terms: Terms = serde_json::from_str(legacy_json).unwrap();
            assert_eq!(terms.liquidation_cvl, CVLPct::Finite(dec!(105)));
            assert_eq!(terms.margin_call_cvl, CVLPct::Infinite);
        }
    }

    #[test]
    fn loan_cvl_pct_scale() {
        let cvl = CVLPct::Finite(dec!(140));
        let value = UsdCents::from(100000);
        let scaled = cvl.scale(value);
        assert_eq!(scaled, UsdCents::try_from_usd(dec!(1400)).unwrap());

        let cvl = CVLPct::Finite(dec!(50));
        let value = UsdCents::from(333333);
        let scaled = cvl.scale(value);
        assert_eq!(scaled, UsdCents::try_from_usd(dec!(1666.67)).unwrap());
    }

    #[test]
    fn current_cvl_from_loan_amounts() {
        let expected_cvl = CVLPct::Finite(dec!(125));
        let collateral_value = UsdCents::from(125000);
        let outstanding_amount = UsdCents::from(100000);
        let cvl = CVLPct::from_loan_amounts(collateral_value, outstanding_amount);
        assert_eq!(cvl, expected_cvl);

        let expected_cvl = CVLPct::Finite(dec!(75));
        let collateral_value = UsdCents::from(75000);
        let outstanding_amount = UsdCents::from(100000);
        let cvl = CVLPct::from_loan_amounts(collateral_value, outstanding_amount);
        assert_eq!(cvl, expected_cvl);
    }

    #[test]
    fn current_cvl_for_zero_amounts() {
        let expected_cvl = CVLPct::ZERO;
        let collateral_value = UsdCents::ZERO;
        let outstanding_amount = UsdCents::from(100000);
        let cvl = CVLPct::from_loan_amounts(collateral_value, outstanding_amount);
        assert_eq!(cvl, expected_cvl);

        let expected_cvl = CVLPct::Infinite;
        let collateral_value = UsdCents::from(75000);
        let outstanding_amount = UsdCents::ZERO;
        let cvl = CVLPct::from_loan_amounts(collateral_value, outstanding_amount);
        assert_eq!(cvl, expected_cvl);

        let expected_cvl = CVLPct::ZERO;
        let collateral_value = UsdCents::ZERO;
        let outstanding_amount = UsdCents::ZERO;
        let cvl = CVLPct::from_loan_amounts(collateral_value, outstanding_amount);
        assert_eq!(cvl, expected_cvl);
    }

    #[test]
    fn cvl_is_significantly_higher() {
        let buffer = CVLPct::new(5);

        let collateral_value = UsdCents::from(125000);
        let outstanding_amount = UsdCents::from(100000);
        let cvl = CVLPct::from_loan_amounts(collateral_value, outstanding_amount);
        let collateral_value = UsdCents::from(130999);
        let outstanding_amount = UsdCents::from(100000);
        let slightly_higher_cvl = CVLPct::from_loan_amounts(collateral_value, outstanding_amount);
        assert!(!cvl.is_significantly_lower_than(slightly_higher_cvl, buffer));
        let collateral_value = UsdCents::from(131000);
        let outstanding_amount = UsdCents::from(100000);
        let significantly_higher_cvl =
            CVLPct::from_loan_amounts(collateral_value, outstanding_amount);
        assert!(cvl.is_significantly_lower_than(significantly_higher_cvl, buffer));
    }
}
