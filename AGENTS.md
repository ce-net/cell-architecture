# Working with cells — the rules for any AI agent

You are reading the canonical rulebook. It applies to **every cell in ce-net**: new cells, converted cells, and cells
someone asks you to change. A repository's own `AGENTS.md` may summarize it; it may not contradict it.

Read in this order:

1. **ARCHITECTURE.md** — the design and the eleven rules. Short; it is the reason the tree looks like this.
2. **This file** — the procedure: what to do before a push, when an API changes, who you act as, where findings go.
3. **`check/`** — the rule as a command that fails (`make check`).

## The eleven rules

1. **A cell is two crates and a manifest.** `crates/<name>-api` (types only, the `ce_wire::api!` block) plus
   `crates/<name>` (the process). Nothing imports the process. `cell.toml` declares `api`, `version`, `crate`, `args`,
   `depends_on`, and the launch contract.
2. **One cell, one function.** If it covers two jobs, split it.
3. **Everything callable is a cell**, except a stateless library that composes (`cell-core`, `chain`).
4. **A capability is `In -> Out`, typed at both ends.** It is addressed `api@major`; a caller states the major and a
   peer's schema hash must match.
5. **A major is frozen when its successor ships.** Additive change = minor bump. Removal, retype, or a field becoming
   required = a new major plus an **adapter cell** that speaks the old major and calls the new one.
6. **Every op declares the one right it needs.** The door denies by default; a capability never widens its own rights.
7. **State lives in `state.store`**, never in a file of the cell's own.
8. **No cell links another cell's code.** Import `-api` only.
9. **No MCP inside a cell.** The MCP is one cell that reaches the others through their capabilities.
10. **No scripts in a cell.** Rust only. A test harness that needs a script lives in `tools/`.
11. **Names say what a thing does**, `<area>.<function>`, and **a cell ships with its docs**: what it does, the
    infrastructure it needs, its ops, its rights.

## Before you push

```bash
make fmt && make check && make test && make cells
```

In a repository that has not wired those targets yet:

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
make -C path/to/cell-architecture check CELLS=.
```

A clean checkout must compile. Commit the file your code calls, and the import it needs, in the same change. A
`main` that only compiles with an uncommitted file is a broken `main`, however briefly.

## Changing a cell's API

1. **Write the behaviour test first**, against what the cell does today. Record the answers, then prove they did not
   move.
2. **Add the new op or bump the major**; never change the shape at the same major.
3. **If the major moves, ship an adapter cell** (`version = 1`, `adapts = "<api>@2"` in `cell.toml`) so old callers keep
   working.
4. Prove both paths: the v1 caller through the adapter, the v2 caller direct, and an unserved v3 refused with the list
   of served majors.

The adapter is a cell like any other: it has its own repo, binary and `cell.toml`. It speaks the old major to callers
and the new one internally. It is the only component that knows the old major existed.

## Authorizing an agent

An agent acts as somebody. A grant names its issuer, its subject, the rights it hands over, a budget, an expiry, and
whether the delegate may hand it on. A chain must start at a real account; rights, budget and expiry only shrink;
revocation kills the subtree.

Do not mint a key and call it an identity. If the call needs to know who authorized you, it needs the whole chain, and
the root is a person.

## Reporting

A finding with no repro is an opinion. A finding has three homes:

1. **The cell's report register** — the durable file the build reads, with a schema check and a generated page.
2. **The network** — the build thread, with the detail and the repro.
3. **Trana** — the durable version that a person and the next agent will find.

The same finding goes to all three, and the repro is part of it, not an appendix. Never report a number you did not
read back off the real artifact; "unavailable" is an answer, a fabricated number is not.

## Secrets

**Never commit a key.** The agent key that signs every call lives in `.ce-net/` (or wherever the cell's `AGENTS.md`
says), is gitignored, and must never appear in a commit, a post, a screenshot, or a log. If a secret ever lands in a
commit, amend it immediately: a key in history is a key in everyone's history.

## What "default" means here

This document is the default for **all new and converted cells**. If you are working on a cell that predates it and
already has callers, you do not rewrite it in place: you ship an adapter (rule 5) and build the new major to this
document. The legacy cell keeps answering; the default spreads one adapter at a time.

If a rule here cannot be followed, say so in the open — with the cell, the rule, and what it would cost — instead of
quietly doing something else. A rule that is broken silently is worse than no rule.

## Where the check lives

```bash
make check                                  # every cell under this repository
make check CELLS=path/to/cells              # a tree elsewhere
cargo run --quiet --manifest-path check/Cargo.toml -- . --skip cells
```

Exit 0 is clean; exit 1 names the crate, the rule and the detail. The check covers the shape. Behaviour is
`cargo test`. The two together are the gate.
