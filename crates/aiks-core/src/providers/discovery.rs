use super::SessionSummary;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryDiagnostic {
    pub code: String,
    pub store_index: usize,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct DiscoveryReport {
    pub sessions: Vec<SessionSummary>,
    pub complete: bool,
    pub diagnostics: Vec<DiscoveryDiagnostic>,
    /// Canonical roots actually covered, never paths inferred from transcript cwd.
    /// Empty preserves the legacy complete-provider discovery contract.
    pub covered_paths: Vec<PathBuf>,
}
impl Default for DiscoveryReport {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            complete: true,
            diagnostics: Vec::new(),
            covered_paths: Vec::new(),
        }
    }
}
impl DiscoveryReport {
    pub fn incomplete(&mut self, code: &str, store_index: usize) {
        self.complete = false;
        if let Some(d) = self
            .diagnostics
            .iter_mut()
            .find(|d| d.code == code && d.store_index == store_index)
        {
            d.count += 1;
        } else {
            self.diagnostics.push(DiscoveryDiagnostic {
                code: code.to_owned(),
                store_index,
                count: 1,
            });
        }
    }
}
