# cell-architecture

**The default shape of a cell in ce-net.** One page, one check, one rulebook — so that the next cell, in the next
tree, by the next agent, is built the same way.

A cell is **two crates and a manifest**:

```text
crates/<name>-api/    types only — the only thing another cell may import
crates/<name>/        the process — nobody imports it, ever
cell.toml             api, version, crate, args, depends_on, launch contract
```

A capability is `In -> Out`, typed at both ends, addressed `api@major`, gated by one right, with its state in
`state.store`. A major is frozen when its successor ships; a break gets an adapter cell.

**Status:** in force from 27 Oct 2026 for all new and converted ce-net cells. Cells that are not converted keep
answering; they reach the default through an adapter, not a rewrite.
**Owner:** Leif Rydenfalk (ce-net). **Discussed on Trana:** see [NETWORK.md](NETWORK.md).

## Adopt it in three steps

```bash
git clone https://github.com/ce-net/cell-architecture
make -C cell-architecture check CELLS=path/to/your/cells   # the shape, as a command that fails
```

1. **Read [ARCHITECTURE.md](ARCHITECTURE.md).** It is the source of truth, and it is short.
2. **Make the check pass** for the cells you are converting: `make check`.
3. **Read [AGENTS.md](AGENTS.md) before you change anything.** It is the same rules written as procedure for an agent,
   plus what to do before a push, when an API changes, who an agent acts as, and where findings go.

If you have a cell that already has callers and does not match: **do not rewrite it in place.** Ship an adapter
(`version = 1`, `adapts = "<api>@2"`) and build the new major to this document — ARCHITECTURE §10.

## What is here

| | |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | the design: cell shape, capability model, versions and adapters, auth chains, the core library, the eleven rules, the loop, migration |
| [AGENTS.md](AGENTS.md) | the rules as procedure for any AI agent: read order, the eleven rules, before a push, API changes, authorizing an agent, reporting, secrets |
| [standards/a-cell-is-two-crates.md](standards/a-cell-is-two-crates.md) | the same rule in the short standard form the ce-fleet cockpit serves |
| [check/](check/) | the architecture rule, executable (Rust, no dependencies); exit 0 clean, 1 broken |
| [NETWORK.md](NETWORK.md) | the Trana posts this came from, and the graph edges between them |
| [LICENSE](LICENSE) | MIT |

## The one rule, in one sentence

A caller compiles against types and calls a **process by name**, so a cell can be rebuilt, replaced, restarted or moved
to another machine without recompiling anything that calls it — and the call is still typed end to end.

## Make target

```bash
make check          # every cell under this repository
```

## Provenance

This repository is the canonical copy, published 27 Oct 2026. It was distilled from the working tree `ce-net cells`
(the same rules, written for one workspace) after the owner made them the default for the whole network. Where this
document and a cell's code disagree, the check is right and the document is the bug — fix the document in the commit
that proves it.
