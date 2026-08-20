//! Translated from `src/nvim/api/private/validate.c` in full.

use crate::api::private::defs::{
    Array, Error, ErrorType, Object, ObjectType,
};

fn set_validation_error(err: &mut Error, message: String) {
    err.r#type = ErrorType::Validation;
    err.msg = Some(message);
}

/// The public API name for an object type (`api_typename`).
#[must_use]
pub fn api_typename(object_type: ObjectType) -> &'static str {
    match object_type {
        ObjectType::Nil => "nil",
        ObjectType::Boolean => "Boolean",
        ObjectType::Integer => "Integer",
        ObjectType::Float => "Float",
        ObjectType::String => "String",
        ObjectType::Array => "Array",
        ObjectType::Dict => "Dict",
        ObjectType::LuaRef => "Function",
        ObjectType::Buffer => "Buffer",
        ObjectType::Window => "Window",
        ObjectType::Tabpage => "Tabpage",
    }
}

/// Set an `"Invalid ..."` API validation error (`api_err_invalid`).
pub fn api_err_invalid(
    err: &mut Error,
    name: &str,
    value: Option<&str>,
    number: i64,
    quote_value: bool,
) {
    let has_space = name.contains(' ');
    let message = match value {
        Some("") => {
            if has_space {
                format!("Invalid {name}")
            } else {
                format!("Invalid '{name}'")
            }
        }
        None => {
            if has_space {
                format!("Invalid {name}: {number}")
            } else {
                format!("Invalid '{name}': {number}")
            }
        }
        Some(value) => match (has_space, quote_value) {
            (true, true) => format!("Invalid {name}: '{value}'"),
            (true, false) => format!("Invalid {name}: {value}"),
            (false, true) => format!("Invalid '{name}': '{value}'"),
            (false, false) => format!("Invalid '{name}': {value}"),
        },
    };
    set_validation_error(err, message);
}

/// Set an `"Invalid ...: expected ..."` validation error
/// (`api_err_exp`).
pub fn api_err_exp(
    err: &mut Error,
    name: &str,
    expected: &str,
    actual: Option<&str>,
) {
    let has_space = name.contains(' ');
    let label = if has_space {
        format!("Invalid {name}")
    } else {
        format!("Invalid '{name}'")
    };
    let message = actual.map_or_else(
        || format!("{label}: expected {expected}"),
        |actual| format!("{label}: expected {expected}, got {actual}"),
    );
    set_validation_error(err, message);
}

/// Set a `"Required: ..."` validation error (`api_err_required`).
pub fn api_err_required(err: &mut Error, name: &str) {
    let message = if name.contains(' ') {
        format!("Required: {name}")
    } else {
        format!("Required: '{name}'")
    };
    set_validation_error(err, message);
}

/// Set a mutually-exclusive-argument validation error
/// (`api_err_conflict`).
pub fn api_err_conflict(err: &mut Error, name: &str, other: &str) {
    let message = if other.contains(' ') {
        format!("Conflict: '{name}' not allowed with {other}")
    } else {
        format!("Conflict: '{name}' not allowed with '{other}'")
    };
    set_validation_error(err, message);
}

/// Validate that every array item is a String and, optionally,
/// contains no newline (`check_string_array`).
pub fn check_string_array(
    array: &Array,
    name: &str,
    disallow_nl: bool,
    err: &mut Error,
) -> bool {
    let item_name = format!("'{name}' item");
    for item in array {
        let Object::String(value) = item else {
            api_err_exp(
                err,
                &item_name,
                api_typename(ObjectType::String),
                Some(api_typename(item.object_type())),
            );
            return false;
        };
        if disallow_nl && value.contains(&b'\n') {
            set_validation_error(
                err,
                format!("'{name}' item contains newlines"),
            );
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_err_invalid_formats_empty_number_and_string_values() {
        for (name, value, number, quote, expected) in [
            ("buffer", Some(""), 0, false, "Invalid 'buffer'"),
            ("buffer id", Some(""), 0, false, "Invalid buffer id"),
            ("buffer", None, 7, false, "Invalid 'buffer': 7"),
            ("buffer id", None, 7, false, "Invalid buffer id: 7"),
            (
                "option",
                Some("bad"),
                0,
                true,
                "Invalid 'option': 'bad'",
            ),
            (
                "option",
                Some("bad"),
                0,
                false,
                "Invalid 'option': bad",
            ),
            (
                "option value",
                Some("bad"),
                0,
                true,
                "Invalid option value: 'bad'",
            ),
            (
                "option value",
                Some("bad"),
                0,
                false,
                "Invalid option value: bad",
            ),
        ] {
            let mut err = Error::default();
            api_err_invalid(&mut err, name, value, number, quote);
            assert_eq!(err.r#type, ErrorType::Validation);
            assert_eq!(err.msg.as_deref(), Some(expected));
        }
    }

    #[test]
    fn api_err_exp_formats_optional_actual_type() {
        let mut err = Error::default();
        api_err_exp(&mut err, "line", "Integer", Some("String"));
        assert_eq!(
            err.msg.as_deref(),
            Some("Invalid 'line': expected Integer, got String")
        );

        api_err_exp(&mut err, "line range", "Integer", None);
        assert_eq!(
            err.msg.as_deref(),
            Some("Invalid line range: expected Integer")
        );
    }

    #[test]
    fn required_and_conflict_messages_preserve_name_quoting_rules() {
        let mut err = Error::default();
        api_err_required(&mut err, "buffer");
        assert_eq!(err.msg.as_deref(), Some("Required: 'buffer'"));
        api_err_required(&mut err, "buffer name");
        assert_eq!(err.msg.as_deref(), Some("Required: buffer name"));

        api_err_conflict(&mut err, "row", "column");
        assert_eq!(
            err.msg.as_deref(),
            Some("Conflict: 'row' not allowed with 'column'")
        );
        api_err_conflict(&mut err, "row", "screen column");
        assert_eq!(
            err.msg.as_deref(),
            Some("Conflict: 'row' not allowed with screen column")
        );
    }

    #[test]
    fn check_string_array_accepts_strings_and_optional_newlines() {
        let array = vec![
            Object::String(b"one".to_vec()),
            Object::String(b"two\nthree".to_vec()),
        ];
        let mut err = Error::default();
        assert!(check_string_array(&array, "lines", false, &mut err));
        assert!(!err.is_set());
    }

    #[test]
    fn check_string_array_rejects_non_string_items() {
        let array = vec![
            Object::String(b"one".to_vec()),
            Object::Integer(2),
        ];
        let mut err = Error::default();
        assert!(!check_string_array(&array, "lines", false, &mut err));
        assert_eq!(
            err.msg.as_deref(),
            Some("Invalid 'lines' item: expected String, got Integer")
        );
    }

    #[test]
    fn check_string_array_rejects_newlines_when_requested() {
        let array = vec![Object::String(b"one\ntwo".to_vec())];
        let mut err = Error::default();
        assert!(!check_string_array(&array, "lines", true, &mut err));
        assert_eq!(
            err.msg.as_deref(),
            Some("'lines' item contains newlines")
        );
    }
}
