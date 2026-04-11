pub(crate) fn render_footer(
    state: impl Into<String>,
    next_step: impl Into<String>,
    optional_follow_up: Option<String>,
) -> Vec<String> {
    let mut lines = vec![
        format!("State: {}", state.into()),
        format!("Next step: {}", next_step.into()),
    ];

    if let Some(optional_follow_up) = optional_follow_up {
        lines.push(format!("Optional follow-up: {optional_follow_up}"));
    }

    lines
}
