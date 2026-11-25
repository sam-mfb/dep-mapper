//! What the preview shows, apart from how it is served.

/// The last good page and the problems found since. Every change has a new
/// version, and every page a new page version, so a browser that missed
/// changes can tell whether its page is still the latest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    version: u64,
    page_version: u64,
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
        self.page_version = self.version;
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

    /// The version at which the last good page was shown; 0 before any.
    pub fn page_version(&self) -> u64 {
        self.page_version
    }

    pub fn page(&self) -> Option<&str> {
        self.page.as_deref()
    }

    pub fn problems(&self) -> &[String] {
        &self.problems
    }

    /// What a browser is told on each change, as JSON:
    /// `{"version": 3, "page": 2, "problems": ["..."]}`. `page` is the page
    /// version, or `null` before any page; a browser showing another page
    /// version reloads, whatever the problems.
    pub fn status_json(&self) -> String {
        serde_json::json!({
            "version": self.version,
            "page": self.page.is_some().then_some(self.page_version),
            "problems": self.problems,
        })
        .to_string()
    }
}

#[cfg(test)]
mod tests;
