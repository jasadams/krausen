// SPDX-License-Identifier: AGPL-3.0-or-later

//! Presentation-only correlation for asynchronous shared preview requests.

#[derive(Debug, Default, Clone)]
pub struct PreviewGate {
    generation: u64,
    accepted_context: Option<String>,
}

impl PreviewGate {
    /// Call on every draft or equipment edit and for every new preview request.
    pub fn invalidate(&mut self) -> u64 {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("preview generation exhausted");
        self.accepted_context = None;
        self.generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        generation == self.generation
    }

    /// Accept only the newest response and, where one was explicitly expected,
    /// the same server-produced calculation context.
    pub fn accept(
        &mut self,
        generation: u64,
        context: &str,
        expected_context: Option<&str>,
    ) -> bool {
        if generation != self.generation
            || context.is_empty()
            || expected_context.is_some_and(|expected| expected != context)
        {
            return false;
        }
        self.accepted_context = Some(context.to_owned());
        true
    }

    pub fn accepted_context(&self) -> Option<&str> {
        self.accepted_context.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::PreviewGate;

    #[test]
    fn old_or_different_context_results_cannot_replace_current_preview() {
        let mut gate = PreviewGate::default();
        let old = gate.invalidate();
        let current = gate.invalidate();
        assert!(!gate.accept(old, "old-context", None));
        assert!(!gate.accept(current, "changed-context", Some("expected-context")));
        assert_eq!(gate.accepted_context(), None);
        assert!(gate.accept(current, "expected-context", Some("expected-context")));
        assert_eq!(gate.accepted_context(), Some("expected-context"));
        gate.invalidate();
        assert_eq!(gate.accepted_context(), None);
        assert!(!gate.accept(current, "expected-context", None));
    }
}
