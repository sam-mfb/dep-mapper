//! A supplier-only query over pinned rustc built MIR. Supported value transfers
//! are indexed before any expected answer is read. Emit may-paths with witnesses.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::{env, fs};

type Local = usize;
type Position = (usize, Local);

#[derive(Clone, Debug)]
struct Operand {
    local: Local,
    fields: Vec<usize>,
}

fn operand(text: &str) -> Option<Operand> {
    let mut s = text.trim();
    for prefix in ["move ", "copy "] {
        s = s.strip_prefix(prefix).unwrap_or(s);
    }
    s = s.strip_prefix('&').unwrap_or(s);
    // Only dereferences, reads of fields and borrows. Reject indexes, variants,
    // computations and arbitrary text containing a local somewhere.
    fn place(s: &str) -> Option<(Operand, &str)> {
        if let Some(s) = s.strip_prefix("(*") {
            let (p, tail) = place(s)?;
            return Some((p, tail.strip_prefix(')')?));
        }
        if let Some(s) = s.strip_prefix('(') {
            let (mut p, tail) = place(s)?;
            let digits = tail.strip_prefix('.')?;
            let (field, ty) = digits.split_once(": ")?;
            let mut nested = 0;
            for (i, c) in ty.char_indices() {
                if c == '(' {
                    nested += 1;
                }
                if c == ')' {
                    if nested == 0 {
                        p.fields.push(field.parse().ok()?);
                        return Some((p, &ty[i + 1..]));
                    }
                    nested -= 1;
                }
            }
            return None;
        }
        let digits = s.strip_prefix('_')?;
        let n = digits.bytes().take_while(u8::is_ascii_digit).count();
        Some((
            Operand {
                local: digits[..n].parse().ok()?,
                fields: vec![],
            },
            &digits[n..],
        ))
    }
    let (p, tail) = place(s)?;
    tail.is_empty().then_some(p)
}

fn strip_generics(path: &str) -> String {
    let mut result = String::new();
    let mut depth = 0;
    let mut prev = ' ';
    for c in path.chars() {
        match c {
            '<' => depth += 1,
            '>' if prev != '-' && depth > 0 => depth -= 1,
            _ if depth == 0 => result.push(c),
            _ => {}
        }
        prev = c;
    }
    result
        .replace("::::", "::")
        .trim_end_matches("::")
        .to_owned()
}

fn qualify(owner: &str, path: &str, crates: &BTreeSet<String>) -> String {
    if crates.contains(path.split("::").next().unwrap()) {
        path.into()
    } else {
        format!("{owner}::{path}")
    }
}

#[derive(Debug)]
struct Call {
    name: String,
    args: Vec<Option<Operand>>,
    dispatch: Option<(String, String)>,
}
#[derive(Debug)]
enum Op {
    Pass(Operand),
    Cast(Operand, String),
    Pack(Vec<(String, Option<Operand>)>),
    Call(Call),
    Opaque,
}
#[derive(Debug)]
struct Line {
    dest: Local,
    op: Op,
    at: String,
}
struct Body {
    owner: String,
    name: String,
    location: Option<String>,
    closure: Option<String>,
    types: BTreeMap<Local, String>,
    lines: Vec<Line>,
    defs: BTreeMap<Local, usize>,
}
fn closure_key(s: &str) -> Option<String> {
    let start = s.find("{closure@")?;
    let end = s[start..].find('}')?;
    Some(s[start..=start + end].into())
}

// MIR can print generic types, tuples and quoted constants inside an operand.
// Only a top-level comma separates actual arguments or aggregate fields.
fn split_at_top(s: &str, delimiter: char) -> Option<Vec<&str>> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut closing = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut previous = ' ';
    for (i, ch) in s.char_indices() {
        if quoted {
            if !escaped && ch == '"' {
                quoted = false;
            }
            escaped = !escaped && ch == '\\';
        } else if ch == '"' {
            quoted = true;
        } else if ch == delimiter && closing.is_empty() {
            pieces.push(&s[start..i]);
            start = i + ch.len_utf8();
        } else {
            match ch {
                '(' => closing.push(')'),
                '[' => closing.push(']'),
                '{' => closing.push('}'),
                '<' => closing.push('>'),
                '>' if previous == '-' => {}
                ')' | ']' | '}' | '>' if closing.pop() != Some(ch) => return None,
                _ => {}
            }
        }
        previous = ch;
    }
    if quoted || !closing.is_empty() {
        return None;
    }
    pieces.push(&s[start..]);
    Some(pieces)
}

fn invocation(s: &str) -> Option<(&str, Vec<Option<Operand>>)> {
    // Search from the last open parenthesis until the argument list balances.
    for (i, _) in s.rmatch_indices('(') {
        if let Some(args) = split_at_top(&s[i + 1..], ',') {
            return Some((&s[..i], args.into_iter().map(operand).collect()));
        }
    }
    None
}

fn operation(rhs: &str, owner: &str, crates: &BTreeSet<String>) -> Op {
    if let Some((called, _)) = rhs.split_once(") -> [") {
        if let Some((raw, args)) = invocation(called) {
            let dispatch = raw
                .strip_prefix('<')
                .and_then(|s| s.rsplit_once(">::"))
                .and_then(|(prefix, method)| {
                    prefix
                        .strip_prefix("dyn ")
                        .and_then(|s| s.rsplit_once(" as "))
                        .map(|(_, tr)| (qualify(owner, &strip_generics(tr), crates), method.into()))
                });
            let name = if raw.starts_with("<{closure@") && raw.ends_with(">::call") {
                closure_key(raw).unwrap_or_default()
            } else if raw.starts_with('<') {
                raw.into()
            } else {
                qualify(owner, &strip_generics(raw), crates)
            };
            return Op::Call(Call {
                name,
                dispatch,
                args,
            });
        }
    }
    if let Some((_, fields)) = rhs.rsplit_once(" { ")
        && let Some(fields) = fields.strip_suffix(" }")
        && let Some(fields) = split_at_top(fields, ',')
        && let Some(entries) = fields
            .into_iter()
            .map(str::trim)
            .map(|f| f.split_once(": ").map(|(k, v)| (k.to_owned(), operand(v))))
            .collect::<Option<Vec<_>>>()
    {
        return Op::Pack(entries);
    }
    if let Some((from, to)) = rhs.split_once(" as ")
        && let Some((to, _)) = to.split_once(" (PointerCoercion(Unsize,")
        && let Some(p) = operand(from)
    {
        return Op::Cast(p, to.into());
    }
    operand(rhs).map(Op::Pass).unwrap_or(Op::Opaque)
}

fn load(path: &Path, owner: &str, crates: &BTreeSet<String>) -> Option<Body> {
    let data = fs::read_to_string(path).unwrap();
    let sig = data.lines().find(|l| l.starts_with("fn "))?;
    let name = data.lines().next()?.split('`').nth(1)?;
    let location = name
        .split_once("<impl at ")
        .and_then(|(_, s)| s.split_once(">::"))
        .and_then(|(s, _)| s.split_once(": "))
        .map(|(begin, _)| begin.to_owned());
    let mut b = Body {
        owner: owner.into(),
        name: format!("{owner}::{name}"),
        location,
        closure: if sig.contains("(_1: &{closure@") {
            closure_key(sig)
        } else {
            None
        },
        types: BTreeMap::new(),
        lines: vec![],
        defs: BTreeMap::new(),
    };
    for line in data.lines() {
        let line = line.trim();
        if let Some(var) = line.strip_prefix("let ") {
            let var = var.strip_prefix("mut ").unwrap_or(var);
            if let Some((l, ty)) = var.split_once(": ")
                && let Some(local) = operand(l)
                && let Some((ty, _)) = ty.split_once(';')
            {
                b.types.insert(local.local, ty.into());
            }
        }
        let Some((inst, comment)) = line.split_once("; ") else {
            continue;
        };
        let Some((dst, rhs)) = inst.split_once(" = ") else {
            continue;
        };
        let Some(pos) = operand(dst) else { continue };
        if !pos.fields.is_empty() || !dst.starts_with('_') {
            continue;
        }
        *b.defs.entry(pos.local).or_default() += 1;
        let Some((_, at)) = comment.split_once(" at ") else {
            continue;
        };
        b.lines.push(Line {
            dest: pos.local,
            op: operation(rhs, owner, crates),
            at: at.into(),
        });
    }
    Some(b)
}

struct Index {
    bodies: Vec<Body>,
    crates: BTreeSet<String>,
    targets: BTreeMap<String, Vec<usize>>,
    callers: BTreeMap<usize, Vec<(usize, usize)>>,
}
impl Index {
    fn read(dir: &Path) -> Self {
        let mut files = vec![];
        let mut crates = BTreeSet::new();
        for folder in fs::read_dir(dir).unwrap() {
            let folder = folder.unwrap().path();
            let name = folder
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .split('-')
                .next()
                .unwrap()
                .to_owned();
            crates.insert(name.clone());
            for item in fs::read_dir(folder).unwrap() {
                let p = item.unwrap().path();
                if p.to_string_lossy().ends_with(".built.after.mir") {
                    files.push((p, name.clone()));
                }
            }
        }
        files.sort();
        let bodies: Vec<_> = files
            .iter()
            .filter_map(|(p, n)| load(p, n, &crates))
            .collect();
        let mut targets = BTreeMap::<String, Vec<usize>>::new();
        for (i, b) in bodies.iter().enumerate() {
            targets.entry(b.name.clone()).or_default().push(i);
            if let Some(k) = &b.closure {
                targets.entry(k.clone()).or_default().push(i);
            }
        }
        let mut callers = BTreeMap::<usize, Vec<_>>::new();
        for (i, b) in bodies.iter().enumerate() {
            for (j, l) in b.lines.iter().enumerate() {
                if let Op::Call(c) = &l.op
                    && let Some(ids) = targets.get(&c.name)
                    && ids.len() == 1
                {
                    callers.entry(ids[0]).or_default().push((i, j));
                }
            }
        }
        Self {
            bodies,
            crates,
            targets,
            callers,
        }
    }
    fn imports(&self, supplier: &str) -> Vec<(usize, usize)> {
        let prefix = format!("{supplier}::");
        self.bodies
            .iter()
            .enumerate()
            .flat_map(|(i, b)| {
                b.lines.iter().enumerate().filter_map({
                    let prefix = &prefix;
                    move |(j, l)| match &l.op {
                        Op::Call(c) if b.owner != supplier && c.name.starts_with(prefix) => {
                            Some((i, j))
                        }
                        _ => None,
                    }
                })
            })
            .collect()
    }
}

#[derive(Clone, Ord, PartialOrd, Eq, PartialEq)]
struct State {
    at: Position,
    fields: Vec<usize>,
    stack: Vec<(usize, usize)>,
}

fn remaining<'a>(p: &Operand, state: &'a State) -> Option<&'a [usize]> {
    (p.local == state.at.1)
        .then(|| state.fields.strip_prefix(p.fields.as_slice()))
        .flatten()
}

struct Search {
    paths: BTreeSet<String>,
    cutoffs: BTreeSet<String>,
    unresolved: BTreeSet<String>,
}

impl Index {
    fn trace(
        &self,
        seed: State,
        contract: &str,
        method: &str,
        history: Vec<String>,
        result: &mut Search,
    ) {
        let mut queue = VecDeque::from([(seed, history)]);
        let mut visited = BTreeSet::new();
        while let Some((s, h)) = queue.pop_front() {
            if !visited.insert(s.clone()) {
                continue;
            }
            let b = &self.bodies[s.at.0];
            // Summarize this reachable consumer once per seed/state, keeping a witness.
            if s.stack.len() > 12 || s.fields.len() > 8 || visited.len() > 25000 {
                result.cutoffs.insert(format!("search limit in {}", b.name));
                continue;
            }
            if b.defs.get(&s.at.1).copied().unwrap_or(0) > 1 {
                result
                    .cutoffs
                    .insert(format!("multiple assignments to _{} in {}", s.at.1, b.name));
                continue;
            }
            let mut enqueue = |at, fields, stack, event: Option<String>| {
                let mut steps = h.clone();
                if let Some(e) = event {
                    steps.push(e);
                }
                queue.push_back((State { at, fields, stack }, steps));
            };
            if s.at.1 == 0 {
                let mut stack = s.stack.clone();
                let callers = match stack.pop() {
                    Some(site) => vec![site],
                    None => self.callers.get(&s.at.0).cloned().unwrap_or_default(),
                };
                for (i, j) in callers {
                    let l = &self.bodies[i].lines[j];
                    enqueue(
                        (i, l.dest),
                        s.fields.clone(),
                        stack.clone(),
                        Some(format!("return to {} @ {}", self.bodies[i].name, l.at)),
                    );
                }
            }
            for (j, l) in b.lines.iter().enumerate() {
                match &l.op {
                    Op::Pass(op) | Op::Cast(op, _) => {
                        if let Some(rest) = remaining(op, &s) {
                            enqueue((s.at.0, l.dest), rest.to_vec(), s.stack.clone(), None);
                        }
                    }
                    Op::Pack(fields) => {
                        for (n, (name, op)) in fields.iter().enumerate() {
                            if let Some(rest) = op.as_ref().and_then(|o| remaining(o, &s)) {
                                let mut path = vec![n];
                                path.extend(rest);
                                enqueue(
                                    (s.at.0, l.dest),
                                    path,
                                    s.stack.clone(),
                                    Some(format!("store {name} @ {}", l.at)),
                                );
                            }
                        }
                    }
                    Op::Call(c) => {
                        for (n, op) in c.args.iter().enumerate() {
                            let Some(rest) = op.as_ref().and_then(|o| remaining(o, &s)) else {
                                continue;
                            };
                            if n == 0
                                && rest.is_empty()
                                && let Some((tr, m)) = &c.dispatch
                                && tr == contract
                                && m == method
                            {
                                let mut steps = h.clone();
                                steps.push(format!("invoked by {} @ {}", b.name, l.at));
                                result.paths.insert(steps.join("\n  "));
                                continue;
                            }
                            if let Some(ids) = self.targets.get(&c.name)
                                && ids.len() == 1
                            {
                                let mut stack = s.stack.clone();
                                stack.push((s.at.0, j));
                                enqueue(
                                    (ids[0], n + 1),
                                    rest.to_vec(),
                                    stack,
                                    Some(format!(
                                        "pass to {} @ {}",
                                        self.bodies[ids[0]].name, l.at
                                    )),
                                );
                            }
                        }
                    }
                    Op::Opaque => {}
                }
            }
        }
    }
}

fn concrete(ty: &str) -> &str {
    let ty = ty.trim_start_matches('&');
    for holder in ["std::boxed::Box<", "std::sync::Arc<"] {
        if let Some(inner) = ty.strip_prefix(holder).and_then(|s| s.strip_suffix('>')) {
            return inner;
        }
    }
    ty
}

fn main() {
    let args: Vec<_> = env::args().collect();
    assert!(
        (4..=5).contains(&args.len()),
        "find MIR-DIRECTORY SUPPLIER owners|paths [IMPL-INDEX]"
    );
    let ix = Index::read(Path::new(&args[1]));
    let supplier = args[2].replace('-', "_");
    assert!(
        ix.crates.contains(&supplier),
        "supplier absent from this build"
    );
    let imports = ix.imports(&supplier);
    if args[3] == "owners" {
        let owners: BTreeSet<_> = imports
            .iter()
            .filter(|(i, _)| ix.bodies[*i].location.is_some())
            .map(|(i, _)| &ix.bodies[*i].owner)
            .collect();
        for owner in owners {
            println!("{owner}");
        }
        return;
    }
    assert_eq!(args[3], "paths");
    let table = fs::read_to_string(&args[4]).unwrap();
    let mut impls = BTreeMap::new();
    for row in table.lines() {
        let v: Vec<_> = row.split('\t').collect();
        impls.insert(
            (v[0], format!("{}:{}:{}", v[1], v[2], v[3]), v[6]),
            (v[4], v[5]),
        );
    }
    let mut result = Search {
        paths: BTreeSet::new(),
        cutoffs: BTreeSet::new(),
        unresolved: BTreeSet::new(),
    };
    let mut seeds = 0;
    for (i, j) in &imports {
        let b = &ix.bodies[*i];
        let l = &b.lines[*j];
        let Some(loc) = &b.location else { continue };
        let method = b.name.rsplit("::").next().unwrap();
        let Some(&(adapter, contract)) = impls.get(&(b.owner.as_str(), loc.clone(), method)) else {
            continue;
        };
        let Op::Call(c) = &l.op else { unreachable!() };
        for (k, injector) in ix.bodies.iter().enumerate() {
            for cast in &injector.lines {
                let Op::Cast(from, target_type) = &cast.op else {
                    continue;
                };
                let Some(ty) = injector.types.get(&from.local) else {
                    continue;
                };
                if !from.fields.is_empty()
                    || qualify(&injector.owner, concrete(ty), &ix.crates) != adapter
                {
                    continue;
                }
                let Some(object) = concrete(target_type).strip_prefix("dyn ") else {
                    continue;
                };
                let spelled = qualify(&injector.owner, object, &ix.crates);
                if spelled.split("::").next() != contract.split("::").next()
                    || spelled.rsplit("::").next() != contract.rsplit("::").next()
                {
                    continue;
                }
                seeds += 1;
                let history = vec![
                    format!(
                        "{} -> {} implements {contract}::{method} @ {}",
                        c.name, adapter, l.at
                    ),
                    format!("supply {adapter} in {} @ {}", injector.name, cast.at),
                ];
                let previous = result.paths.len();
                ix.trace(
                    State {
                        at: (k, cast.dest),
                        fields: vec![],
                        stack: vec![],
                    },
                    contract,
                    method,
                    history,
                    &mut result,
                );
                if result.paths.len() == previous {
                    result.unresolved.insert(format!(
                        "{adapter} as {contract}::{method} at {} (no invocation reached)",
                        cast.at
                    ));
                }
            }
        }
    }
    for path in &result.paths {
        println!("PATH {path}");
    }
    eprintln!(
        "{} compiled functions; {} direct supplier calls; {seeds} starting points; {} path witnesses; {} search cutoffs",
        ix.bodies.len(),
        imports.len(),
        result.paths.len(),
        result.cutoffs.len()
    );
    for reason in result.unresolved {
        eprintln!("unresolved: {reason}");
    }
    for cutoff in result.cutoffs {
        eprintln!("cutoff: {cutoff}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_constants_do_not_shift_call_arguments() {
        let (callee, args) =
            invocation("example::f::<fn()>((const \"x, y\", const 0), move _7").unwrap();
        assert_eq!(callee, "example::f::<fn()>");
        assert_eq!(args.len(), 2);
        assert!(args[0].is_none());
        assert_eq!(args[1].as_ref().unwrap().local, 7);
    }

    #[test]
    fn reading_another_field_cannot_deliver_the_tracked_value() {
        let state = State {
            at: (0, 1),
            fields: vec![2, 4],
            stack: vec![],
        };
        let right = operand("&(*((*_1).2: &Example))").unwrap();
        let wrong = operand("&(*((*_1).3: &Example))").unwrap();
        assert_eq!(remaining(&right, &state), Some(&[4][..]));
        assert_eq!(remaining(&wrong, &state), None);
    }
}
