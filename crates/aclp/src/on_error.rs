use serde::{Deserialize, Serialize};

/// What a failed request does to the harness continuation it guards.
///
/// A request fails on a transport failure, the deadline, a mismatched id, an invalid or oversized
/// result or a JSON-RPC error. Under either policy the harness reports the failure through
/// diagnostics. Notifications never guard a continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnError {
    /// The continuation proceeds as if the result carried no effects.
    Allow,
    /// The continuation is prevented.
    Deny,
}

impl OnError {
    /// The policy for `method` when the integration sets none: `Deny` for `tool.requested` and
    /// `permission.requested`, `Allow` for every other method.
    #[must_use]
    pub fn default_for(method: &str) -> Self {
        match method {
            "tool.requested" | "permission.requested" => OnError::Deny,
            _ => OnError::Allow,
        }
    }

    /// The policy as configuration spells it: `allow` or `deny`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            OnError::Allow => "allow",
            OnError::Deny => "deny",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OnError;

    #[test]
    fn only_tool_and_permission_guards_fail_closed_by_default() {
        assert_eq!(OnError::default_for("tool.requested"), OnError::Deny);
        assert_eq!(OnError::default_for("permission.requested"), OnError::Deny);

        for method in [
            "input.submitted",
            "model.requested",
            "client.registered",
            "client.updated",
            "interaction.requested",
        ] {
            assert_eq!(OnError::default_for(method), OnError::Allow, "{method}");
        }
    }

    #[test]
    fn configuration_spells_the_policy_in_lowercase() {
        let allow = serde_json::to_string(&OnError::Allow).expect("serializes");
        assert_eq!(allow, "\"allow\"");
        let deny: OnError = serde_json::from_str("\"deny\"").expect("parses");
        assert_eq!(deny, OnError::Deny);
        assert!(serde_json::from_str::<OnError>("\"open\"").is_err());
        assert_eq!(OnError::Deny.as_str(), "deny");
    }
}
