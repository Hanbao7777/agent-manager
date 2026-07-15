pub fn next_sequence_from_task_ids<I, S>(ids: I) -> u64
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    ids.into_iter()
        .filter_map(|id| {
            id.as_ref()
                .strip_prefix("install-")
                .and_then(|value| value.parse::<u64>().ok())
        })
        .max()
        .and_then(|value| value.checked_add(1))
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resumes_after_the_highest_persisted_generated_id() {
        let ids = ["custom", "install-2", "install-17", "install-invalid"];
        assert_eq!(next_sequence_from_task_ids(ids), 18);
    }

    #[test]
    fn fresh_store_starts_at_one() {
        assert_eq!(next_sequence_from_task_ids(std::iter::empty::<&str>()), 1);
    }
}
