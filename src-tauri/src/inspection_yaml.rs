//! Bounded YAML inspection without implicit string coercion or external values.
use crate::error::{AppError, AppResult};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::{collections::BTreeMap, fmt};

pub(crate) const MAX_YAML_BYTES: usize = 2_000_000;

/// Inspection needs structure, strings and booleans, but must not reinterpret
/// numbers/null as strings or coerce non-string keys into field names.
#[derive(Debug, PartialEq)]
pub(crate) enum Value {
    Scalar,
    Bool(bool),
    String(String),
    Sequence(Vec<Value>),
    Mapping(BTreeMap<String, Value>),
}

impl Value {
    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        self.as_mapping()?.get(key)
    }

    pub(crate) fn as_mapping(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Self::Mapping(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }
}

struct StringKey(String);

impl<'de> Deserialize<'de> for StringKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeyVisitor;
        impl Visitor<'_> for KeyVisitor {
            type Value = StringKey;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string YAML mapping key")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StringKey(value.to_owned()))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(StringKey(value))
            }
        }
        // deserialize_str would allow the parser to coerce numeric/bool keys.
        deserializer.deserialize_any(KeyVisitor)
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ValueVisitor;
        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a YAML value with string mapping keys")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
                Ok(Value::Scalar)
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
                Ok(Value::Bool(value))
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Value, E> {
                Ok(Value::Scalar)
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Value, E> {
                Ok(Value::Scalar)
            }
            fn visit_i128<E: de::Error>(self, _: i128) -> Result<Value, E> {
                Ok(Value::Scalar)
            }
            fn visit_u128<E: de::Error>(self, _: u128) -> Result<Value, E> {
                Ok(Value::Scalar)
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Value, E> {
                Ok(Value::Scalar)
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
                Ok(Value::String(value.to_owned()))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
                Ok(Value::String(value))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element()? {
                    values.push(value);
                }
                Ok(Value::Sequence(values))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut mapping: A) -> Result<Value, A::Error> {
                let mut values = BTreeMap::new();
                while let Some((StringKey(key), value)) = mapping.next_entry()? {
                    if values.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate YAML mapping key"));
                    }
                }
                Ok(Value::Mapping(values))
            }
        }
        deserializer.deserialize_any(ValueVisitor)
    }
}

pub(crate) fn parse(text: &str) -> AppResult<Value> {
    if text.len() > MAX_YAML_BYTES {
        return Err(AppError::Invalid(
            "azure.yaml exceeds the inspection size limit.".into(),
        ));
    }
    let options = serde_saphyr::options! {
        strict_booleans: true,
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::Error,
        // Retain the previous inspection semantics: merge values are traversed
        // as ordinary nested values, not expanded into surrounding properties.
        merge_keys: serde_saphyr::MergeKeyPolicy::AsOrdinary,
        reject_unsupported_tags: true,
        non_finite_float_policy: serde_saphyr::NonFiniteFloatPolicy::Reject,
        with_snippet: false,
        alias_limits: serde_saphyr::alias_limits! {
            max_total_replayed_events: 100_000,
            max_replay_stack_depth: 32,
            max_alias_expansions_per_anchor: 128,
        },
        budget: serde_saphyr::budget! {
            max_reader_input_bytes: Some(MAX_YAML_BYTES),
            max_depth: 64,
            flow_nesting_limit: 64,
            max_documents: 1,
            max_nodes: 100_000,
            max_events: 200_000,
            max_total_scalar_bytes: MAX_YAML_BYTES,
            max_anchors: 128,
            max_aliases: 512,
            max_recorded_anchor_events: 100_000,
            max_recorded_anchor_bytes: MAX_YAML_BYTES,
        },
    };
    Ok(serde_saphyr::from_str_with_options(text, options)?)
}

#[cfg(test)]
mod tests {
    use super::{parse, Value, MAX_YAML_BYTES};

    #[test]
    fn preserves_primitive_types_and_literal_azd_substitutions() {
        let value = parse("name: 12\nversion: '12'\ninteractive: true\ntext: 'true'\nno: off\nsubstitution: ${AZURE_ENV_NAME}\n").unwrap();
        assert_eq!(value.get("name"), Some(&Value::Scalar));
        assert_eq!(value.get("version").and_then(Value::as_str), Some("12"));
        assert_eq!(
            value.get("interactive").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(value.get("text").and_then(Value::as_bool), None);
        assert_eq!(value.get("no").and_then(Value::as_str), Some("off"));
        assert_eq!(
            value.get("substitution").and_then(Value::as_str),
            Some("${AZURE_ENV_NAME}")
        );
    }

    #[test]
    fn accepts_bounded_aliases_without_expanding_merge_properties() {
        let value =
            parse("defaults: &defaults {interactive: true}\nhooks: {predeploy: {<<: *defaults}}\n")
                .unwrap();
        let hook = value.get("hooks").unwrap().get("predeploy").unwrap();
        assert_eq!(hook.get("interactive"), None);
        assert_eq!(
            hook.get("<<")
                .unwrap()
                .get("interactive")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn rejects_ambiguous_or_external_yaml_instead_of_weakening_inspection() {
        for text in [
            "hooks: {}\nhooks: {predeploy: {interactive: true}}",
            "true: {interactive: true}",
            "12: {interactive: true}",
            "? [hooks, predeploy]\n: {interactive: true}",
            "hooks: !custom {predeploy: {interactive: true}}",
            "hooks: !include ./other.yaml",
            "value: .nan",
            "name: first\n---\nname: second",
            "hooks: [",
        ] {
            assert!(parse(text).is_err(), "unexpectedly accepted: {text}");
        }
    }

    #[test]
    fn rejects_depth_size_and_alias_amplification() {
        assert!(parse(&format!("value: {}0{}", "[".repeat(70), "]".repeat(70))).is_err());
        assert!(parse(&"x".repeat(MAX_YAML_BYTES + 1)).is_err());
        let aliases = "*hook, ".repeat(129);
        assert!(parse(&format!(
            "hook: &hook {{interactive: true}}\nhooks: [{aliases}]"
        ))
        .is_err());
    }
}
