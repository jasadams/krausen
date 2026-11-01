// SPDX-License-Identifier: AGPL-3.0-or-later
//! Retain request identities independently for uncertain session revocations.
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub struct SessionRevokeAttempts(BTreeMap<String, String>);

impl SessionRevokeAttempts {
    pub fn request(&self, token: &str) -> Option<String> {
        self.0.get(token).cloned()
    }

    pub fn insert(&mut self, token: String, request: String) {
        self.0.insert(token, request);
    }

    pub fn clear_matching(&mut self, token: &str, request: &str) {
        if self.0.get(token).is_some_and(|current| current == request) {
            self.0.remove(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lost_response_for_a_survives_b_completion() {
        let mut attempts = SessionRevokeAttempts::default();
        attempts.insert("A".into(), "request-A".into());
        attempts.insert("B".into(), "request-B".into());
        attempts.clear_matching("B", "request-B");
        assert_eq!(attempts.request("A").as_deref(), Some("request-A"));
        attempts.insert("B".into(), "new-B".into());
        attempts.clear_matching("B", "request-B");
        assert_eq!(attempts.request("B").as_deref(), Some("new-B"));
    }
}
