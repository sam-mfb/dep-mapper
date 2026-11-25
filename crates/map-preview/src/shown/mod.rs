//! What the preview shows, apart from how it is served.

/// The last good page and the problems found since. Every change has a new
/// version, so a browser can tell what it has missed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    version: u64,
    page: Option<String>,
    problems: Vec<String>,
}

impl Shown {
    /// Nothing to show yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A newly built page. It replaces the last one and clears the problems.
    pub fn show_page(&mut self, page: String) {
        self.page = Some(page);
        self.problems.clear();
        self.version += 1;
    }

    /// Problems that kept a new page from being built. The last good page
    /// stays.
    pub fn show_problems(&mut self, problems: Vec<String>) {
        self.problems = problems;
        self.version += 1;
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn page(&self) -> Option<&str> {
        self.page.as_deref()
    }

    pub fn problems(&self) -> &[String] {
        &self.problems
    }

    /// What a browser is told on each change, as JSON:
    /// `{"version": 3, "page": true, "problems": ["..."]}`.
    pub fn status_json(&self) -> String {
        serde_json::json!({
            "version": self.version,
            "page": self.page.is_some(),
            "problems": self.problems,
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests;
