use super::call::Caller;

/// One audited write: the tool, how it ended, who asked through which client, and the app's own
/// name for what it touched.
#[derive(Debug, Clone, Copy)]
pub struct Event<'a> {
    /// The tool, or `pending` for the confirm page.
    pub tool: &'a str,
    /// How it ended, such as `ok`, `error`, `input_required`, `requested` or `done`.
    pub outcome: &'a str,
    /// Who asked.
    pub caller: &'a Caller,
    /// What it touched, in the app's words.
    pub target: &'a str,
}

/// Logs one write at info level under the `mcp` target. It never logs arguments or content, so
/// a mail body or an event title never reaches the logs. The suite's logger ships to Journal.
pub fn audit(event: Event<'_>) {
    tracing::info!(
        target: "mcp",
        tool = event.tool,
        outcome = event.outcome,
        user_id = event.caller.user_id.as_str(),
        client_id = event.caller.client_id.as_str(),
        actor = event.caller.actor.as_str(),
        target = event.target,
        "mcp write"
    );
}
