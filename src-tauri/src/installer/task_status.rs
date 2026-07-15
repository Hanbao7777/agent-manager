#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutcome {
    Succeeded,
    Failed,
    InstalledNotRunnable,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregatedTaskStatus {
    Succeeded,
    InstalledNotRunnable,
    NeedsUserAction,
    Failed,
}

pub fn aggregate_tool_outcomes<I>(outcomes: I) -> AggregatedTaskStatus
where
    I: IntoIterator<Item = ToolOutcome>,
{
    let outcomes = outcomes.into_iter().collect::<Vec<_>>();
    if outcomes
        .iter()
        .all(|outcome| *outcome == ToolOutcome::Succeeded)
    {
        return AggregatedTaskStatus::Succeeded;
    }
    let failed = outcomes
        .iter()
        .filter(|outcome| **outcome == ToolOutcome::Failed)
        .count();
    let installed_not_runnable = outcomes
        .iter()
        .any(|outcome| *outcome == ToolOutcome::InstalledNotRunnable);
    let succeeded = outcomes
        .iter()
        .any(|outcome| *outcome == ToolOutcome::Succeeded);
    let skipped = outcomes
        .iter()
        .any(|outcome| *outcome == ToolOutcome::Skipped);

    if failed == outcomes.len() {
        AggregatedTaskStatus::Failed
    } else if installed_not_runnable && failed == 0 && !skipped {
        AggregatedTaskStatus::InstalledNotRunnable
    } else if failed > 0 || skipped || succeeded {
        AggregatedTaskStatus::NeedsUserAction
    } else {
        AggregatedTaskStatus::Failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_failed_is_a_failed_task() {
        assert_eq!(
            aggregate_tool_outcomes([ToolOutcome::Failed, ToolOutcome::Failed]),
            AggregatedTaskStatus::Failed
        );
    }

    #[test]
    fn not_runnable_has_a_distinct_task_status() {
        assert_eq!(
            aggregate_tool_outcomes([ToolOutcome::Succeeded, ToolOutcome::InstalledNotRunnable,]),
            AggregatedTaskStatus::InstalledNotRunnable
        );
    }

    #[test]
    fn partial_failure_needs_user_action() {
        assert_eq!(
            aggregate_tool_outcomes([ToolOutcome::Succeeded, ToolOutcome::Failed]),
            AggregatedTaskStatus::NeedsUserAction
        );
    }
}
