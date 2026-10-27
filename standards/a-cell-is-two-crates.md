# a-cell-is-two-crates

**When it applies:** every new or converted cell in ce-net, in every repo, on every machine. A cell that already has
callers and predates this rule reaches it through an adapter cell, not a rewrite.
**Author:** ce-net, 27 Oct 2026. **Owner of the rule:** Leif Rydenfalk.
**Source of record:** <https://github.com/ce-net/cell-architecture/blob/main/ARCHITECTURE.md>.

Leif, 2026-10-27, verbatim:

> "Post this so that this is the default on all of ce-net with the architecture and not just here locally... default for
> all new and converted ce-net cells and we use adapter cells and adapter architecture for the rest."

## The rule

1. **A cell is two crates and a manifest.** `crates/<name>-api` (types only, the `ce_wire::api!` block) plus
   `crates/<name>` (the process) plus `cell.toml`. Nothing imports the process. The `-api` crate is the only thing
   another cell may import.
2. **One cell, one function.** If it covers two jobs, split it.
3. **Everything callable is a cell**, except a stateless library that composes (`cell-core`, `chain`).
4. **A capability is `In -> Out`, typed at both ends**, addressed `api@major`. The caller states the major; a peer's
   schema hash must match. Additive change = minor bump.
5. **A major is frozen when its successor ships.** Removal, retype, or a field becoming required = a new major plus an
   **adapter cell** (`version = 1`, `adapts = "<api>@2"`) that speaks the old major and calls the new one.
6. **Every op declares the one right it needs.** The door denies by default; a capability never widens its own rights.
7. **State lives in `state.store`**, never in a file of the cell's own.
8. **No cell links another cell's code.** Import `-api` only.
9. **No MCP inside a cell.** The MCP is one cell that reaches the others through their capabilities.
10. **No scripts in a cell.** Rust only. A harness that needs one lives in `tools/`.
11. **Names say what a thing does** (`<area>.<function>`), and **a cell ships with its docs**: what it does, the
    infrastructure it needs, its ops, its rights.

## Why the two crates are not a style choice

If a caller links a cell's code, rebuilding that cell recompiles every caller, a panic in it crashes them, and it
cannot move to another machine. With the split, a caller compiles against types once and the call crosses the wire. It
is still typed end to end because both sides are generated from the same `-api` crate, and a schema hash refuses a
mismatched peer at the Hello.

Rules 1 and 8 are the same rule seen from two sides: **types cross, processes do not.**

## Proof it is done

The shape is a command that fails:

```bash
git clone https://github.com/ce-net/cell-architecture
make -C cell-architecture check CELLS=path/to/cells   # exit 0 = clean
```

It fails on a cell with no `-api` crate, an `-api` crate that depends on a process, any crate that depends on a cell's
binary, an `-api` crate with a non-infrastructure dependency, a cell with no declared version, and an adapter that
adapts to itself. Behaviour is `cargo test`; the two together are the gate.

## Where it is enforced

| gate | what it proves |
|---|---|
| `make cells` / `make check` (the `check/` Rust binary) | the shape of every cell under the tree |
| `make check` / CI | a clean checkout compiles |
| `make test` | the cell behaves as documented |
| the adapter demo | v1 → adapter → v2 works, an unserved v3 is refused |

A rule that is not in that table is advice, not a rule.
