//! The text of pieces and handle details.

use super::{ClusterPlan, span_text};
use graph_evaluator::Clusters;
use project_graph::{
    Dispatch, Endpoint, FactId, FactKind, ImportUse, ItemId, ItemKind, ModuleId, ProjectGraph,
    ReceiverBasis, RouteEnd, RouteId, SourceSpan, Support, Visibility,
};
use std::collections::BTreeMap;
use std::fmt::Write;

/// How many of a route step's facts its summary shows.
const SHOWN_FACTS: usize = 3;

/// How much of a route's facts its description names.
#[derive(Clone, Copy)]
pub(super) enum Length {
    /// Every use site and fact, for the route's own handle.
    Full,
    /// The first few of each list, for pieces that list many routes.
    Summary,
}

/// Converts a span's byte offsets into line numbers, when the source is available.
pub trait SourceLines {
    /// The first and last line, counting from 1.
    fn lines(&self, span: &SourceSpan) -> Option<(usize, usize)>;
}

/// Reports no lines, so spans are shown as byte offsets.
pub struct ByteOffsets;

impl SourceLines for ByteOffsets {
    fn lines(&self, _span: &SourceSpan) -> Option<(usize, usize)> {
        None
    }
}

pub(super) struct Renderer<'a> {
    graph: &'a ProjectGraph,
    source: &'a dyn SourceLines,
    command: Option<&'a str>,
    clusters: &'a ClusterPlan,
}

impl<'a> Renderer<'a> {
    pub(super) fn new(
        graph: &'a ProjectGraph,
        source: &'a dyn SourceLines,
        command: Option<&'a str>,
        clusters: &'a ClusterPlan,
    ) -> Self {
        Renderer {
            graph,
            source,
            command,
            clusters,
        }
    }

    pub(super) fn module_name(&self, id: &ModuleId) -> String {
        match self.graph.modules.get(id) {
            Some(module) => module.name.clone(),
            None => id.0.clone(),
        }
    }

    /// A module's page: its units, its gaps and the whole graph's, and a summary of its routes grouped
    /// by where they lead, then its items, paged. The routes themselves are
    /// listed in full in the routes pieces the summary names.
    pub(super) fn module_pages(
        &self,
        id: &ModuleId,
        routes: &[&RouteId],
        piece_of: &BTreeMap<RouteId, String>,
        page_size: usize,
    ) -> Vec<String> {
        let graph = self.graph;
        let module = &graph.modules[id];
        let mut head = String::new();
        let _ = writeln!(head, "Module {} (module:{})", module.name, id.0);
        let _ = writeln!(head, "Root: {}", module.root);
        let _ = write!(head, "{}", self.module_clusters(id));
        let units: Vec<_> = graph
            .units
            .iter()
            .filter(|(_, u)| &u.module == id)
            .collect();
        let _ = writeln!(head, "\nBuild units:");
        if units.is_empty() {
            let _ = writeln!(head, "- none recorded");
        }
        for (unit_id, unit) in &units {
            let settings: Vec<String> = unit
                .settings
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            let _ = writeln!(
                head,
                "- {}: target {}{}",
                unit_id.0,
                unit.target,
                if settings.is_empty() {
                    String::new()
                } else {
                    format!(", {}", settings.join(", "))
                }
            );
        }
        let gaps: Vec<_> = graph
            .gaps
            .iter()
            .filter(|gap| {
                gap.unit
                    .as_ref()
                    .is_some_and(|u| units.iter().any(|(id, _)| *id == u))
            })
            .collect();
        // Gaps without a unit, such as routes left out of the whole graph,
        // bear on every module's judgment, so every module page shows them.
        let whole_graph: Vec<_> = graph.gaps.iter().filter(|gap| gap.unit.is_none()).collect();
        for (heading, gaps) in [
            ("Coverage gaps", gaps),
            ("Coverage gaps for the whole graph", whole_graph),
        ] {
            if gaps.is_empty() {
                continue;
            }
            let _ = writeln!(head, "\n{heading}:");
            for gap in gaps {
                let at = gap
                    .site
                    .as_ref()
                    .map(|s| format!(" at {}", span_text(self.source, s)));
                let _ = writeln!(head, "- {}{}", gap.reason, at.unwrap_or_default());
            }
        }
        let record = match self.command {
            Some(c) => format!(
                "\nTo record this module's decision:\n  {c} judge module:{m} --concern <text> --kind logic|effect|composition|contract --cluster <name> --reason <text> --cite <handle> [--cite <handle>...]\n  {c} defer module:{m} --reason <text>\nRun `{c} show <handle>` to see any route, fact or item in full.\n",
                m = id.0
            ),
            None => String::new(),
        };
        let _ = write!(head, "{}", self.route_groups(id, routes, piece_of));
        let items: Vec<String> = graph
            .items
            .keys()
            .filter(|i| &graph.items[*i].module == id)
            .map(|item| self.item_line(item))
            .collect();
        paged(&head, "Items", &items, page_size, &record)
    }

    /// The routes touching a module, counted by the module or outside
    /// package at their other end, with the piece that lists them and a few
    /// examples.
    fn route_groups(
        &self,
        id: &ModuleId,
        routes: &[&RouteId],
        piece_of: &BTreeMap<RouteId, String>,
    ) -> String {
        const EXAMPLES: usize = 3;
        let mut groups: BTreeMap<(String, String), Vec<&RouteId>> = BTreeMap::new();
        for route in routes {
            let other = self.other_end(id, route);
            let piece = piece_of.get(*route).cloned().unwrap_or_default();
            groups.entry((other, piece)).or_default().push(route);
        }
        let mut text = format!(
            "\nRoutes that start, pass through or end in this module: {}\n",
            routes.len()
        );
        if groups.is_empty() {
            text.push_str("- none\n");
        }
        let mut ordered: Vec<_> = groups.into_iter().collect();
        ordered.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        for ((other, piece), group) in ordered {
            let examples: Vec<String> = group
                .iter()
                .take(EXAMPLES)
                .map(|r| format!("route:{}", r.0))
                .collect();
            let _ = writeln!(
                text,
                "- {} {other}, listed in {piece}; for example {}",
                group.len(),
                examples.join(", ")
            );
        }
        text
    }

    /// Where a route leads from a module's point of view: another project
    /// module, an outside package, or nowhere beyond the module.
    fn other_end(&self, id: &ModuleId, route: &RouteId) -> String {
        let route = &self.graph.routes[route];
        let points = std::iter::once(&route.origin).chain(route.hops.iter().map(|hop| &hop.to));
        let mut outside = None;
        for point in points {
            match point {
                Endpoint::Module(m) if m != id => return format!("with {}", self.module_name(m)),
                Endpoint::Item(i) | Endpoint::Place { owner: i, .. } => {
                    if let Some(item) = self.graph.items.get(i)
                        && &item.module != id
                    {
                        return format!("with {}", self.module_name(&item.module));
                    }
                }
                Endpoint::External(name) | Endpoint::Unresolved(name) if outside.is_none() => {
                    outside = Some(outside_package(name));
                }
                _ => {}
            }
        }
        match outside {
            Some(package) => format!("leaving for {package}"),
            None => "within the module".into(),
        }
    }

    pub(super) fn routes_pages(
        &self,
        modules: &[ModuleId],
        routes: &[RouteId],
        page_size: usize,
    ) -> Vec<String> {
        let names: Vec<String> = modules
            .iter()
            .map(|m| format!("{} (module:{})", self.module_name(m), m.0))
            .collect();
        let head = match names.as_slice() {
            [one] => {
                format!("Routes that stay in {one} or leave it for points outside the project\n")
            }
            _ => format!("Routes between {}\n", names.join(" and ")),
        };
        let piece = modules
            .iter()
            .map(|m| m.0.as_str())
            .collect::<Vec<_>>()
            .join("+");
        let record = match self.command {
            Some(c) => format!(
                "\nTo record what these routes mean:\n  {c} relate routes/{piece} --kind direct-use|implements|wires|supplies-input|full-injection|adapted-injection|incidental|other:<name> --reason <text> --cite <handle> [--cite <handle>...]\n  {c} skip-route <route handle> --reason <text>   (considered, carries no relationship)\nA relationship may cite routes from other pieces too.\n"
            ),
            None => String::new(),
        };
        let details: Vec<String> = routes
            .iter()
            .map(|r| self.route_detail(r, Length::Summary))
            .collect();
        paged(&head, "Routes", &details, page_size, &record)
    }

    /// What a module piece says about clusters: the fixed list, if there is
    /// one, and the module's earlier cluster, if known.
    fn module_clusters(&self, id: &ModuleId) -> String {
        let mut text = String::new();
        if let Some(fixed) = self.clusters.fixed_clusters() {
            let _ = writeln!(
                text,
                "\nThe clusters are fixed, from top to bottom: {}. Name one of them. If none fits, defer the module and say why.",
                quoted(fixed)
            );
        }
        if let Some(previous) = self.clusters.previous().get(&id.0) {
            let _ = writeln!(
                text,
                "\nAn earlier evaluation put this module in cluster {previous:?}. That is a hint, not evidence."
            );
        }
        text
    }

    pub(super) fn clusters_page(&self) -> String {
        if let Some(fixed) = self.clusters.fixed_clusters() {
            return format!(
                "Clusters\n\nThe clusters are fixed, from top to bottom: {}. Every module judgment names one of them; a module that fits none is deferred with a reason. There is nothing to record here.\n",
                quoted(fixed)
            );
        }
        let ask = "Clusters\n\nName the clusters the modules belong to, from top to bottom: first the cluster of the program, which uses the others, and last the foundations, which use none of them. A module's cluster should sit at or above every cluster it uses. Every cluster a module judgment names must be listed.\n";
        match self.command {
            Some(c) => format!(
                "{ask}`{c} show clusters` lists the module decisions recorded so far.\n\nTo record the list:\n  {c} clusters <name> [<name>...]\nRecording it again replaces it.\n"
            ),
            None => ask.to_string(),
        }
    }

    pub(super) fn item_detail(&self, id: &ItemId) -> String {
        let item = &self.graph.items[id];
        let mut text = format!(
            "item:{}: {} {} in {} (module:{}), {}",
            id.0,
            item_kind(&item.kind),
            item.name,
            self.module_name(&item.module),
            item.module.0,
            visibility(&item.visibility)
        );
        if let Some(signature) = &item.signature {
            let _ = write!(text, "\n  signature: {signature}");
        }
        if let Some(span) = &item.declaration {
            let _ = write!(text, "\n  declared at {}", span_text(self.source, span));
        }
        text
    }

    pub(super) fn fact_detail(&self, id: &FactId) -> String {
        let fact = &self.graph.facts[id];
        let unit = match self.graph.units.get(&fact.unit) {
            Some(unit) => format!("{} in {}", fact.unit.0, self.module_name(&unit.module)),
            None => fact.unit.0.clone(),
        };
        let mut text = format!(
            "fact:{}: {}\n  unit: {unit}\n  subject: {}\n  object: {}",
            id.0,
            fact_kind(&fact.kind),
            self.endpoint(&fact.subject),
            self.endpoint(&fact.object)
        );
        if let Some(site) = &fact.site {
            let _ = write!(text, "\n  at {}", span_text(self.source, site));
        }
        text
    }

    /// A route's description. `Length::Full` names every use site and every
    /// fact; `Length::Summary` names the first few of each list and counts
    /// the rest.
    pub(super) fn route_detail(&self, id: &RouteId, length: Length) -> String {
        let route = &self.graph.routes[id];
        let handle = format!("route:{}", id.0);
        let facts = |facts: &[FactId]| self.fact_list(facts, length, &handle);
        let mut text = handle.clone();
        match route.use_sites.as_slice() {
            [] => text.push_str("\n  no known use"),
            [_] => {
                let _ = write!(text, "\n  explains the use at{}", facts(&route.use_sites));
            }
            sites => {
                let _ = write!(
                    text,
                    "\n  explains the uses at {} sites:{}",
                    sites.len(),
                    facts(sites)
                );
            }
        }
        let _ = write!(text, "\n  from {}", self.endpoint(&route.origin));
        for hop in &route.hops {
            let support = match &hop.support {
                Support::Established => String::new(),
                Support::Possible { condition: Some(c) } => format!(", possible when {c}"),
                Support::Possible { condition: None } => ", possible".into(),
            };
            let _ = write!(
                text,
                "\n  -> {}{support} via{}",
                self.endpoint(&hop.to),
                facts(&hop.facts)
            );
        }
        match &route.end {
            RouteEnd::ReachedUse => text.push_str("\n  reaches its use"),
            RouteEnd::Stopped { at, reason } => {
                let _ = write!(text, "\n  stops: {reason}; at{}", facts(at));
            }
        }
        text
    }

    /// The facts of one list in a route's description, each with its brief,
    /// to follow a label. One fact stays on the label's line. In full, each
    /// of several facts gets a line of its own. In summary, the first few
    /// follow the label and the rest are counted, with the command that
    /// lists them all when there is one.
    fn fact_list(&self, facts: &[FactId], length: Length, route: &str) -> String {
        let line = |fact: &FactId| format!("fact:{} ({})", fact.0, self.fact_brief(fact));
        match (length, facts) {
            (_, [one]) => format!(" {}", line(one)),
            (Length::Full, facts) => facts.iter().map(|f| format!("\n    {}", line(f))).collect(),
            (Length::Summary, facts) => {
                let shown: Vec<String> = facts.iter().take(SHOWN_FACTS).map(line).collect();
                let shown = shown.join(", ");
                match (facts.len().saturating_sub(SHOWN_FACTS), self.command) {
                    (0, _) => format!(" {shown}"),
                    (more, Some(c)) => {
                        format!(" {shown} and {more} more, which `{c} show {route}` lists")
                    }
                    (more, None) => format!(" {shown} and {more} more"),
                }
            }
        }
    }

    fn item_line(&self, id: &ItemId) -> String {
        let item = &self.graph.items[id];
        format!(
            "item:{}: {} {}, {}{}",
            id.0,
            item_kind(&item.kind),
            item.name,
            visibility(&item.visibility),
            item.signature
                .as_ref()
                .map(|s| format!(": {s}"))
                .unwrap_or_default()
        )
    }

    fn fact_brief(&self, id: &FactId) -> String {
        match self.graph.facts.get(id) {
            Some(fact) => match &fact.site {
                Some(site) => format!(
                    "{} at {}",
                    fact_kind(&fact.kind),
                    span_text(self.source, site)
                ),
                None => fact_kind(&fact.kind),
            },
            None => "not in this graph".into(),
        }
    }

    fn endpoint(&self, endpoint: &Endpoint) -> String {
        let item_name = |id: &ItemId| {
            self.graph
                .items
                .get(id)
                .map(|item| item.name.clone())
                .unwrap_or_else(|| "unknown item".into())
        };
        match endpoint {
            Endpoint::Module(id) => format!("module:{} ({})", id.0, self.module_name(id)),
            Endpoint::Item(id) => format!("item:{} ({})", id.0, item_name(id)),
            Endpoint::Place { owner, projection } => {
                format!("{projection} in item:{} ({})", owner.0, item_name(owner))
            }
            Endpoint::External(name) => format!("outside the project: {name}"),
            Endpoint::Unresolved(name) => format!("unresolved: {name}"),
        }
    }
}

/// The package an outside name belongs to: its first path segment, without
/// reference or generic punctuation.
fn outside_package(name: &str) -> String {
    let trimmed = name.trim_start_matches(['<', '&', '*', ' ']);
    let first = trimmed.split("::").next().unwrap_or(trimmed);
    let first = first.split(['<', ' ', '>']).next().unwrap_or(first);
    if first.is_empty() {
        name.to_string()
    } else {
        first.to_string()
    }
}

/// Splits a list across pages, repeating the head and the closing text on each.
fn paged(
    head: &str,
    heading: &str,
    lines: &[String],
    page_size: usize,
    close: &str,
) -> Vec<String> {
    let chunks: Vec<&[String]> = if lines.is_empty() {
        vec![&[]]
    } else {
        lines.chunks(page_size.max(1)).collect()
    };
    let count = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            let mut page = head.to_string();
            let _ = write!(page, "\n{heading}");
            if count > 1 {
                let _ = write!(page, " (page {} of {count})", index + 1);
            }
            page.push_str(":\n");
            if chunk.is_empty() {
                page.push_str("- none\n");
            }
            for line in chunk {
                let _ = writeln!(page, "- {line}");
            }
            page.push_str(close);
            page
        })
        .collect()
}

fn item_kind(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Type => "type",
        ItemKind::Value => "value",
        ItemKind::Function => "function",
        ItemKind::Method => "method",
        ItemKind::Module => "module",
    }
}

fn visibility(visibility: &Visibility) -> &'static str {
    match visibility {
        Visibility::Exported => "exported",
        Visibility::Internal => "internal",
        Visibility::Unknown => "visibility unknown",
    }
}

fn fact_kind(kind: &FactKind) -> String {
    match kind {
        FactKind::Import { local_name, usage } => format!(
            "import as {local_name} ({})",
            match usage {
                ImportUse::Unused => "unused",
                ImportUse::TypeOnly => "used only as a type",
                ImportUse::Value => "used as a value",
                ImportUse::Undetermined => "use undetermined",
            }
        ),
        FactKind::Reexport { exported_name } => format!("re-export as {exported_name}"),
        FactKind::Call(dispatch) => format!(
            "call ({})",
            match dispatch {
                Dispatch::Direct => "direct",
                Dispatch::SelectedImplementation => "selected implementation",
                Dispatch::Contract => "through a contract",
                Dispatch::Indirect => "indirect",
                Dispatch::Foreign => "foreign",
            }
        ),
        FactKind::ReceiverBound { call, basis } => format!(
            "receiver bound for fact:{} ({})",
            call.0,
            match basis {
                ReceiverBasis::SelectedByCompiler => "selected by the compiler",
                ReceiverBasis::UniqueInClosedBuild => "unique in the closed build",
                ReceiverBasis::BuildCandidate => "a candidate in this build",
            }
        ),
        other => super::fact_kind_slug(other).replace('-', " "),
    }
}

/// Names in quotes, separated by commas.
fn quoted(clusters: &Clusters) -> String {
    clusters
        .iter()
        .map(|cluster| format!("{:?}", cluster.0))
        .collect::<Vec<_>>()
        .join(", ")
}
