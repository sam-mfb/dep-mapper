# What dep-mapper maps

Dep-mapper helps a developer answer “What does this package expose, who has a way to use it, and how does its work reach them?” Its overview is a map of packages. A reader can follow an individual arrow down to the project code that explains it.

## Agreed scope

- Analyze the source of a project for an identified build. Preserve which packages, target and configuration the evidence describes.
- Follow relationships within the project. When a value enters a third-party dependency, record the visible handoff. If it reappears in project code, record the visible receiving site. Joining those sites requires evidence.
- Describe the ways a package is wired for use. Proving that an operation executes for some particular runtime input is outside the goal.

## Three responsibilities

| Responsibility | Decision |
| --- | --- |
| Establish evidence | Deterministic analysis records contract surfaces and arrows. It records what is exposed, where a name is imported or re-exported, where a type is mentioned, where a value is created, stored, passed or returned, and which method or operation is used. Keep the relevant method, value and source location behind each package-level relationship. |
| Interpret evidence | A human or LLM describes a module’s concern, assigns its kind, and labels particular routes or uses. Full injection, adaptation, incidental use, and mixed use are architectural judgments. Two routes between the same packages can receive different labels. |
| Present the interpretation | Draw the labeled modules and routes. Selecting a module reveals its concern, kind, provided surface, dependencies and consumers. Selecting a route reveals its direction, participating modules and supporting evidence, including alternatives or a gap. A label can be revised independently of the extracted facts. |

An unused import, use only in a type position, a public re-export and a call convey different things. Evidence for an unused import must apply to that import in the selected build. The reviewer can decide whether any of these observations indicate poor practice.

For example, kashkeshet calls the backup implementation itself and also passes it into install commands. Both uses belong on the map. In a different case, B might implement C’s interface and call A inside one method. To judge whether B adapts A for C, the reviewer needs to see which method C uses and what that method does with A. An adjacent call to a logger is evidence of a dependency, too.

Keep source-supported handoffs, possible connections, and the place a trace stops distinguishable. A missing route can mean incomplete evidence. The reviewer’s confidence in a label cannot supply a missing handoff.

The module boundaries, dependency arrangements and review workflow for dep-mapper itself are proposals for design review.

## Review decisions and composition

Architecture review is a contract for making an architectural judgment. The request identifies a module or route, the evidence bearing on it, and the decision being requested. Its response ties the judgment to that subject and evidence. A reviewer can leave a subject undecided.

Two proposed effect modules implement the contract. **Architecture review terminal** displays the evidence, asks a human to decide, and returns the answer. **Architecture review LLM** submits the review request to a model API and returns the model’s decision. A different review mechanism can implement the same contract.

**Mapper command** owns the composition. It selects a reader for the project language and supplies it to Dependency evidence through Project observations. The Rust reader serves the initial investigation. The TypeScript reader illustrates how another language can supply the same observations. Mapper command obtains the resulting graph and adapts the relevant evidence to the review contract. It selects a reviewer and calls it through that contract. For publication it pairs decisions with the original evidence and prepares the input document defined by Map viewer. The reviewer owns the decision, Dependency evidence owns the facts, and Map viewer owns their presentation.

**Map files** handles persistence. It can save evidence and completed decisions for reuse and write the published map. Making a fresh decision takes place in the selected reviewer.
