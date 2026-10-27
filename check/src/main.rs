//! cellcheck — the cell architecture of ce-net, enforced instead of remembered.
//!
//! The rule: a cell is two crates and a manifest.
//!
//!     crates/<name>-api/   types only. What other cells are allowed to import.
//!     crates/<name>/       the running process. Nobody imports this, ever.
//!     cell.toml            how the door runs it.
//!
//! A caller compiles against types and calls a *process* by name. No cell links another cell's code, which means a
//! cell can be rebuilt, replaced, restarted or moved to another machine without recompiling anything that calls it,
//! and the call is still typed end to end because both sides speak the same `-api` crate.
//!
//! Canonical rule: <https://github.com/ce-net/cell-architecture> (ARCHITECTURE.md, rules 1 and 8).
//!
//!     cargo run --quiet --manifest-path check/Cargo.toml -- .                 # every cell under here
//!     cargo run --quiet --manifest-path check/Cargo.toml -- doc-store         # one cell
//!     cargo run --quiet --manifest-path check/Cargo.toml -- . --skip cells    # exclude legacy cells
//!
//! Exit 0 when the graph is clean, 1 when it is not.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Crates a cell may depend on without that being a coupling: the transport, the gate, the state client, the runtime.
const INFRA: &[&str] = &[
    "ce-wire", "ce_gate", "ce-gate", "ce-cap", "state-store-api", "state_store_api", "wire-src", "gate-src", "tokio",
    "serde", "serde_json", "sha2", "async-trait", "thiserror", "toml", "anyhow", "tracing", "bytes", "futures", "rand",
    "hex", "base64", "uuid", "chrono", "time", "url", "regex", "indexmap", "postcard",
];

const SKIP: &[&str] = &["target", ".cargo-home", ".git", "node_modules", "__pycache__", ".venv", "ce-audit"];

#[derive(Clone, Debug, Default)]
struct Crate {
    name: String,
    path: PathBuf,
    deps: Vec<String>,
    is_api: bool,
    is_cell: bool,
    api_name: String,
    api_version: Option<i64>,
    adapts: String,
}

#[derive(Clone, Debug)]
struct Problem {
    _krate: String,
    rule: String,
    detail: String,
}

fn main() -> ExitCode {
    let mut skip: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--skip" {
            if let Some(v) = args.next() {
                skip.push(v.trim_end_matches('/').to_string());
            }
        } else {
            rest.push(a);
        }
    }
    let root = PathBuf::from(rest.first().cloned().unwrap_or_else(|| ".".into()));
    let root = root.canonicalize().unwrap_or(root);
    let (crates, mut problems) = check(&root, &skip);

    let cells: Vec<&Crate> = crates.values().filter(|c| c.is_cell).collect();
    let apis: Vec<&Crate> = crates.values().filter(|c| c.is_api).collect();
    let libs: Vec<&Crate> = crates.values().filter(|c| !c.is_cell && !c.is_api).collect();

    println!("{}", root.display());
    println!(
        "  {} cell(s), {} api crate(s), {} library crate(s)",
        cells.len(),
        apis.len(),
        libs.len()
    );
    let api_names: BTreeSet<&str> = apis.iter().map(|c| c.name.as_str()).collect();
    for c in &cells {
        let api = format!("{}-api", c.name);
        let mark = if api_names.contains(api.as_str()) { "ok  " } else { "MISS" };
        let ver = c.api_version.unwrap_or(1);
        let extra = if c.adapts.is_empty() {
            String::new()
        } else {
            format!("  adapts {}", c.adapts)
        };
        println!("  {mark} {:<12}@{:<4} {:<16} api: {}{extra}", c.api_name, ver, c.name, api);
    }
    if !libs.is_empty() {
        let names: Vec<&str> = libs.iter().map(|c| c.name.as_str()).collect();
        println!("  libraries (no state, no door, not cells): {}", names.join(", "));
    }

    problems.sort_by(|a, b| (&a.rule, &a.detail).cmp(&(&b.rule, &b.detail)));
    if problems.is_empty() {
        println!("\nPASS: no cell imports another cell's process; every type crosses the wire.");
        ExitCode::SUCCESS
    } else {
        println!("\nFAIL:");
        for p in &problems {
            println!("  [{}] {}", p.rule, p.detail);
        }
        ExitCode::FAILURE
    }
}

fn check(root: &Path, skip: &[String]) -> (BTreeMap<String, Crate>, Vec<Problem>) {
    let mut files = Vec::new();
    walk(root, &mut files);

    let mut crates: BTreeMap<String, Crate> = BTreeMap::new();
    for f in &files {
        if f.file_name().and_then(|n| n.to_str()) != Some("Cargo.toml") {
            continue;
        }
        if excluded(f, root, skip) {
            continue;
        }
        let Ok(text) = fs::read_to_string(f) else { continue };
        let Some((name, deps)) = parse_cargo(&text) else { continue };
        let dir = f.parent().unwrap_or(root).to_path_buf();
        crates.insert(
            name.clone(),
            Crate { is_api: name.ends_with("-api"), name, path: dir, deps, ..Default::default() },
        );
    }

    let mut problems: Vec<Problem> = Vec::new();
    let mut cells: BTreeMap<String, Crate> = BTreeMap::new();

    for f in &files {
        if f.file_name().and_then(|n| n.to_str()) != Some("cell.toml") {
            continue;
        }
        if excluded(f, root, skip) {
            continue;
        }
        let Ok(text) = fs::read_to_string(f) else { continue };
        let meta = parse_cell(&text);
        let label = if !meta.crate_name.is_empty() { meta.crate_name.clone() } else { meta.api.clone() };

        if meta.version.is_none() {
            problems.push(Problem {
                _krate: label.clone(),
                rule: "cell declares no version".into(),
                detail: format!("{}: add version = <major>; a call names api@major, never a guess", f.display()),
            });
        }
        if !meta.adapts.is_empty() {
            match meta.adapts.split_once('@') {
                Some((a, v)) if v.chars().all(|c| c.is_ascii_digit()) && !v.is_empty() => {
                    let same_major = meta.version.map(|n| n.to_string()).as_deref() == Some(v);
                    if !meta.api.is_empty() && a == meta.api && same_major {
                        problems.push(Problem {
                            _krate: label.clone(),
                            rule: "adapter adapts to itself".into(),
                            detail: format!("{}: {}@{} -> {}", f.display(), meta.api, meta.version.unwrap_or(1), meta.adapts),
                        });
                    }
                }
                _ => problems.push(Problem {
                    _krate: label.clone(),
                    rule: "adapter's adapts is not api@major".into(),
                    detail: format!("{}: adapts = {:?}", f.display(), meta.adapts),
                }),
            }
        }

        // the manifest names its binary: that is the process, wherever the crate lives
        let mut target = crates.get(&meta.crate_name).cloned();
        if target.is_none() {
            let parent = f.parent().unwrap_or(root);
            target = crates
                .values()
                .find(|c| c.path == parent || c.path.parent() == Some(parent))
                .cloned();
        }
        let Some(mut t) = target else {
            problems.push(Problem {
                _krate: label.clone(),
                rule: "cell.toml names no crate".into(),
                detail: format!("{} says crate = {:?}, which is not in the tree", f.display(), meta.crate_name),
            });
            continue;
        };
        t.is_cell = true;
        t.api_name = meta.api.clone();
        t.api_version = meta.version;
        t.adapts = meta.adapts.clone();
        cells.insert(t.name.clone(), t);
    }

    // make the same crates the output and the rules see: a cell found through its manifest is a cell in the tree
    for (n, c) in &cells {
        if let Some(k) = crates.get_mut(n) {
            k.is_cell = true;
            k.api_name = c.api_name.clone();
            k.api_version = c.api_version;
            k.adapts = c.adapts.clone();
        }
    }

    // rule 1: a cell has a types-only api crate, and the api crate does not depend on the cell
    for (name, _cell) in &cells {
        let api_crate = format!("{name}-api");
        let Some(api) = crates.get(&api_crate) else {
            problems.push(Problem {
                _krate: name.clone(),
                rule: "cell has no -api crate".into(),
                detail: format!("expected crates/{api_crate} with the ce_wire::api! block"),
            });
            continue;
        };
        if api.deps.iter().any(|d| d == name) {
            problems.push(Problem {
                _krate: api_crate.clone(),
                rule: "the api crate depends on the cell".into(),
                detail: format!("{api_crate} -> {name} would couple the types to the process"),
            });
        }
    }

    // rule 2: nothing outside a cell's own binary may depend on that binary
    let cell_binaries: BTreeSet<&str> = cells.keys().map(|s| s.as_str()).collect();
    for c in crates.values() {
        for d in &c.deps {
            if cell_binaries.contains(d.as_str()) && c.name != *d {
                problems.push(Problem {
                    _krate: c.name.clone(),
                    rule: "depends on another cell's process".into(),
                    detail: format!("{} -> {} (import only {}-api)", c.name, d, d),
                });
            }
        }
    }

    // rule 3: an api crate depends on ce-wire and nothing that is a cell
    for c in crates.values() {
        if !c.is_api {
            continue;
        }
        for d in &c.deps {
            if cell_binaries.contains(d.as_str()) {
                problems.push(Problem {
                    _krate: c.name.clone(),
                    rule: "api crate depends on a cell process".into(),
                    detail: format!("{} -> {}", c.name, d),
                });
            } else if !INFRA.contains(&d.as_str()) && !d.starts_with("ce-") {
                problems.push(Problem {
                    _krate: c.name.clone(),
                    rule: "api crate has a non-infra dependency".into(),
                    detail: format!("{} -> {} (types only: ce-wire)", c.name, d),
                });
            }
        }
    }

    // rule 4: a library is allowed, but it must not depend on a cell's process
    for c in crates.values() {
        if c.is_api || c.is_cell {
            continue;
        }
        for d in &c.deps {
            if cell_binaries.contains(d.as_str()) {
                problems.push(Problem {
                    _krate: c.name.clone(),
                    rule: "library depends on a cell process".into(),
                    detail: format!("{} -> {}; depend on {}-api", c.name, d, d),
                });
            }
        }
    }

    (crates, problems)
}

/// Every `Cargo.toml` and `cell.toml` under `root`, without following symlinks or descending into skipped dirs.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if SKIP.contains(&name.as_ref()) || name.starts_with('.') {
                continue;
            }
            walk(&path, out);
        } else if name == "Cargo.toml" || name == "cell.toml" {
            out.push(path);
        }
    }
    out.sort();
}

fn excluded(file: &Path, root: &Path, skip: &[String]) -> bool {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let rel = rel.to_string_lossy();
    skip.iter().any(|s| rel.starts_with(&format!("{s}/")) || rel == *s)
}

/// A minimal TOML subset: `[package]`, `[dependencies]`, `[dev-dependencies]`, and `[dependencies.x]` tables.
/// Enough for a Cargo manifest and deliberately no more; the checker must not need a toolchain to run.
fn parse_cargo(text: &str) -> Option<(String, Vec<String>)> {
    let mut section = String::new();
    let mut name: Option<String> = None;
    let mut deps: BTreeSet<String> = BTreeSet::new();
    for raw in text.lines() {
        let line = strip_comment(raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_string();
            if let Some(rest) = section.strip_prefix("dependencies.") {
                deps.insert(key(rest));
            } else if let Some(rest) = section.strip_prefix("dev-dependencies.") {
                deps.insert(key(rest));
            }
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = key(k);
        if section == "package" && k == "name" {
            name = Some(unquote(v.trim()));
        } else if section == "dependencies" || section == "dev-dependencies" {
            deps.insert(k);
        }
    }
    name.map(|n| (n, deps.into_iter().collect()))
}

#[derive(Default)]
struct CellMeta {
    api: String,
    crate_name: String,
    version: Option<i64>,
    adapts: String,
}

fn parse_cell(text: &str) -> CellMeta {
    let mut meta = CellMeta::default();
    let mut in_cell = false;
    for raw in text.lines() {
        let line = strip_comment(raw);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_cell = line[1..line.len() - 1].trim() == "cell";
            continue;
        }
        if !in_cell {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let k = key(k);
        let v = v.trim();
        match k.as_str() {
            "api" => meta.api = unquote(v),
            "crate" => meta.crate_name = unquote(v),
            "version" => meta.version = v.parse::<i64>().ok(),
            "adapts" => meta.adapts = unquote(v),
            _ => {}
        }
    }
    meta
}

/// Drop a trailing `#` comment, but not a `#` inside a quoted value.
fn strip_comment(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            out.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' && quote.is_some() {
            out.push(c);
            escaped = true;
            continue;
        }
        match quote {
            Some(q) => {
                out.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                    out.push(c);
                } else if c == '#' {
                    break;
                } else {
                    out.push(c);
                }
            }
        }
    }
    out
}

fn key(s: &str) -> String {
    s.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').trim_matches('\'').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_manifest_reads_package_and_deps() {
        let t = r#"
            [package]
            name = "doc-store"
            version = "0.1.0"

            [dependencies]
            ce-wire = "0.1"
            serde = { version = "1", features = ["derive"] }
            state-store-api = { path = "../state-store-api" }

            [dev-dependencies]
            tokio = { version = "1", features = ["macros"] }

            [dependencies.sha2]
            version = "0.10"
        "#;
        let (name, deps) = parse_cargo(t).unwrap();
        assert_eq!(name, "doc-store");
        for d in ["ce-wire", "serde", "state-store-api", "tokio", "sha2"] {
            assert!(deps.iter().any(|x| x == d), "missing {d} in {deps:?}");
        }
    }

    #[test]
    fn a_workspace_manifest_is_not_a_crate() {
        assert!(parse_cargo("[workspace]\nmembers = [\"crates/a\"]\n").is_none());
    }

    #[test]
    fn cell_manifest_reads_the_contract() {
        let m = parse_cell(
            r#"
            [cell]
            api = "demo.counter"
            version = 2
            crate = "counter"
            adapts = "demo.counter@1"   # an adapter

            [launch]
            health = "HealthOp"
        "#,
        );
        assert_eq!(m.api, "demo.counter");
        assert_eq!(m.crate_name, "counter");
        assert_eq!(m.version, Some(2));
        assert_eq!(m.adapts, "demo.counter@1");
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        let (name, deps) = parse_cargo("[package]\nname = \"a#b\"\n[dependencies]\nx = \"1\" # real comment\n").unwrap();
        assert_eq!(name, "a#b");
        assert_eq!(deps, vec!["x".to_string()]);
    }

    #[test]
    fn the_check_refuses_a_cell_with_no_api_crate() {
        let tmp = std::env::temp_dir().join(format!("cellcheck-test-{}", std::process::id()));
        let cell = tmp.join("crates").join("broken");
        fs::create_dir_all(&cell).unwrap();
        fs::write(cell.join("Cargo.toml"), "[package]\nname = \"broken\"\nversion = \"0.1.0\"\n").unwrap();
        fs::write(cell.join("cell.toml"), "[cell]\napi = \"demo.broken\"\nversion = 1\n").unwrap();
        let (_, problems) = check(&tmp, &[]);
        assert!(
            problems.iter().any(|p| p.rule == "cell has no -api crate"),
            "expected the negative control to fail: {problems:?}"
        );
        fs::remove_dir_all(&tmp).ok();
    }
}
