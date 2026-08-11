//! Translated from `src/nvim/channel.c`.
//!
//! Channel transport, RPC dispatch and event-loop integration are not
//! translated yet. This module starts with the representation-
//! independent helpers used around that machinery.

/// Three-way comparison of channel IDs (`int64_t_cmp`).
///
/// Written with comparisons rather than subtraction, so the full
/// `i64` range is safe.
#[must_use]
pub fn int64_t_cmp(a: i64, b: i64) -> i32 {
    if a == b {
        0
    } else if a > b {
        1
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int64_t_cmp_reports_all_three_orderings() {
        assert_eq!(int64_t_cmp(3, 2), 1);
        assert_eq!(int64_t_cmp(2, 3), -1);
        assert_eq!(int64_t_cmp(3, 3), 0);
    }

    #[test]
    fn int64_t_cmp_handles_the_full_range_without_subtraction_overflow() {
        assert_eq!(int64_t_cmp(i64::MAX, i64::MIN), 1);
        assert_eq!(int64_t_cmp(i64::MIN, i64::MAX), -1);
    }

    #[test]
    fn int64_t_cmp_drives_ascending_channel_id_sorting() {
        let mut ids = [i64::MAX, 7, -3, i64::MIN, 0];
        ids.sort_by(|a, b| int64_t_cmp(*a, *b).cmp(&0));
        assert_eq!(ids, [i64::MIN, -3, 0, 7, i64::MAX]);
    }
}
