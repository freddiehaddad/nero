//! Translated from `src/nvim/main.c` (startup parser core).
//!
//! Full process startup remains coupled to event-loop, channel, UI,
//! command, and file-loading subsystems. [`get_number_arg`] is an
//! independent command-line parser used by that flow.

/// Parse a decimal number at `argument[*index]` (`get_number_arg`).
///
/// Leaves `default` and `index` unchanged when the next byte is not a
/// digit.
#[must_use]
pub fn get_number_arg(argument: &[u8], index: &mut usize, default: i32) -> i32 {
    if !argument
        .get(*index)
        .is_some_and(u8::is_ascii_digit)
    {
        return default;
    }
    let start = *index;
    while argument
        .get(*index)
        .is_some_and(u8::is_ascii_digit)
    {
        *index += 1;
    }
    std::str::from_utf8(&argument[start..*index])
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_number_arg_parses_digits_and_advances_index() {
        let mut index = 2;
        assert_eq!(get_number_arg(b"-o120tail", &mut index, 1), 120);
        assert_eq!(index, 5);
    }

    #[test]
    fn get_number_arg_keeps_default_for_missing_or_nonnumeric_value() {
        let mut index = 2;
        assert_eq!(get_number_arg(b"-ox", &mut index, 7), 7);
        assert_eq!(index, 2);
        let mut end = 2;
        assert_eq!(get_number_arg(b"-o", &mut end, 9), 9);
        assert_eq!(end, 2);
    }
}
