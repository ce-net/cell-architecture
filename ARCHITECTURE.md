# The cell architecture

**Status:** the default for cells in ce-net, from 27 Oct 2026.
**Owner:** Leif Rydenfalk (the ce-net account that signs the door).
**Canonical source:** <https://github.com/ce-net/cell-architecture>.
**Applies to:** every new cell and every converted cell, in every repo, on every machine. Existing cells that are not
converted keep answering; they reach the default through an adapter, not a rewrite (see §10).

This is the design everything else follows: how a cell is shaped, what a capability is, how versions survive a
breaking change, what the core library enforces, and how fast the loop has to be to keep it maintainable.

It was written in one tree after the same idea was asked for four times in different words. It is published here so
that the next cell, in the next tree, by the next agent, follows the same rule without having to rediscover it.

---

## 0. What a cell is for

A cell exists so that a capability can be **rebuilt, replaced, restarted or moved to another machine without
recompiling anything that calls it** — while the call stays typed at both ends.

That is the whole argument. Everything below is what that sentence costs.

---

## 1. A cell is two crates and a manifest

```text
crates/<name>-api/    types only — the ce_wire::api! block. The only thing another cell may import.
crates/<name>/        the process. Nobody imports it, ever.
cell.toml             api, version, crate, args, depends_on, health, backoff.
```

A `cell.toml` in full:

```toml
[cell]
api = "doc.store"          # <area>.<function>
version = 1                # the major this cell serves
crate = "doc-store"        # the process crate
args = ["serve"]
depends_on = ["state.store@2"]
# adapts = "doc.store@2"   # only on an adapter cell: the major it calls internally

[launch]
health = "HealthOp"
start_timeout_ms = 5000
restart_backoff_ms = [1000, 300000]
```

Why the split is not a style choice: if a caller links a cell's code, rebuilding that cell recompiles every caller, a
panic in it crashes them, and it cannot move to another machine. With the split, a caller compiles against types once
and the call crosses the wire. It is still typed end to end because **both sides are generated from the same `-api`
crate**, and a schema hash refuses a mismatched peer at the Hello.

The check that enforces it is `check/` — a small Rust binary, `make check` (§8). It fails on:

- a cell with no `-api` crate;
- an `-api` crate that depends on a process;
- any crate that depends on a cell's binary;
- an `-api` crate with a non-infrastructure dependency;
- a cell with no declared version;
- an adapter that adapts to itself.

---

## 2. A capability is a function over the network

A capability **is** a function — `In -> Out`, typed at both ends. It is called a capability because it has five things
a local function does not:

| | function | capability |
|---|---|---|
| where it runs | this process | a process chosen at call time, anywhere |
| which shape | whatever this build has | a **major** the caller states, plus a schema hash the peer must match |
| who may call | whoever can link it | a **right** the door checks, default deny |
| how it fails | its error type | a **class** a chain can act on |
| how it composes | compile time only | compile time **and** across the wire |

The naming rule that falls out:

- local, stateless, no door → **a library function**; wrap it with `fn_cap("name", f)` and it is a `Capability`
- across a process, named, versioned, gated → **a cell** (or an adapter cell)

```rust
chain(fn_cap("double", |n: u64| async move { Ok(n * 2) }))
    .then(needs("demo.math:use"))
    .then(call(cell))                 // crosses a process; the chain does not care which is which
```

---

## 3. One primitive, therefore recursion

```rust
trait Capability<In, Out> {
    const NAME: &'static str;
    async fn call(&self, cx: &mut Cx, input: In) -> Result<Out>;
}
```

A cell op is a capability. A right check is a capability. A rate limit is a capability. A trace row is a capability.
**A chain is a capability**, because composition returns a capability:

```text
Capability<A, B> .then(Capability<B, C>)  ->  Capability<A, C>
```

That is what makes a named chain callable by name, nestable inside another chain, gateable, and testable like a cell.
Two guards keep recursion sane, and both are enforced at run time:

| guard | rule | error |
|---|---|---|
| cycle | a name may not appear twice on the call stack | `cycle: a -> b -> a` |
| depth | `cx.depth < cx.max_depth` (default 8, per chain) | `depth 8 at 'doc.write' (max 8)` |

Both unwind on success, on refusal and on a cycle, so a caller can never inherit a poisoned stack.

---

## 4. Versions are in the call; adapters are cells

A capability is addressed **`api@major`**. The caller states it. Nothing guesses, and an unserved major is refused with
the list of what is served:

```text
no provider for demo.counter@3: served majors are [1, 2]
```

An **adapter is a cell**: own repo, own binary, own `cell.toml`, reached by name like any other. It speaks the old major
to callers and calls the new one internally.

```toml
[cell]
api = "demo.counter"
version = 1                   # what it serves
adapts = "demo.counter@2"     # what it calls
```

`cargo run --example version_demo` (in the reference tree's `chain/` crate) runs the whole picture: caller A (v1) →
adapter → cell v2, caller B (v2) → cell v2, caller C (v3) refused. Both old and new work simultaneously, and the
adapter is the only component that knows v1 existed.

**The rule that follows: a major is frozen when its successor ships.** You never edit v1 again; you add an adapter, or
you leave v1 running. That is why the schema hash covers the major and not the minor.

This is also the migration path for everything that predates this document. A legacy cell does not have to be rewritten
to be reachable from the default: it keeps its old shape behind an adapter that speaks the old major, and the new
major is built to the default. §10 has the steps.

---

## 4.5 Auth chains: every call traces back to a person

A key on its own says "somebody minted this". An auth chain says **who authorized whom**, and every chain starts at a
real account:

```text
  acct:leif  ──grant──▶  agent:plover  ──grant──▶  agent:helper  ──▶ call
  (person, browser)         (delegate)              (delegate)
```

```rust
let g1 = Grant::root_grant(&leif.account, &leif.key, &plover.verifying_key(), "agent:plover",
                           &["doc.store@1:write"], 100, now, in_24h);
let g2 = g1.attenuate(&plover, &helper.verifying_key(), "agent:helper",
                      &["doc.store@1:write"], 40, in_6h)?;   // subset only
let id  = AuthChain::new(vec![g1, g2]).verify(now, &accounts, &revoked)?;
let mut cx = Cx::from_identity(&id);          // the only way a real call gets a context
```

Four rules, each one a test in `cell-core`:

| rule | what it stops |
|---|---|
| **the root is an account** | an agent minting its own ancestry: `NotRootedInPerson` |
| **every link is signed by its issuer, and links are contiguous** | a grant edited, or a grant re-addressed to a thief: `BadSignature`, `NotChained` |
| **rights, budget and expiry only shrink** | a delegate handing on more than it holds: `Escalation`, `BudgetEscalation`, `ExpiryEscalation` |
| **revocation kills the subtree** | an old key still working after a person revokes it: `Revoked` |

Why it is a chain and not a claim: the door verifies the whole path, so "who authorized this agent" has one answer that
a person can read — `Identity { account, via, rights, budget, expires_ms }` — and a compromised delegate cannot
manufacture an ancestor.

**What this changes in practice:** `Cx::from_identity` is the only constructor that sets `account`, and a capability can
ask `cx.is_authenticated()` before it does anything expensive. The old free key still works for the agent network — it
just no longer *is* an identity; it is a link in a chain that has to start at somebody.

---

## 5. The core library every cell depends on

`cell-core` — a library, not a cell (no state, no door), so it cannot rot into a shared backend.

**`schema`** — what a peer may be, and what a change costs.

- additive **with** a minor bump → `Minor`; old callers keep working
- additive **without** a bump → `Breaking` (a pinned caller cannot tell the shapes apart)
- removal, retype, or a field becoming required → `Breaking`, at any version
- same major + different hash → `Drift`, refused at the Hello, both hashes named
- different major → `WrongMajor`, both versions named

**`call`** — how it fails, and what may be retried.

| class | retried |
|---|---|
| `Refused`, `NotFound`, `Invalid` | never |
| `Conflict` | never |
| `RateLimited`, `Unavailable`, `Internal` | only if the **op** is idempotent |

`check_policy(policy, idempotent)` refuses the call *before it starts* when attempts > 1 on a non-idempotent op. The
deadline covers every attempt together. A server `retry_after` beats the client's backoff.

**`rollback`** — a saga with a ledger on disk.

- the undo registers only after the forward action succeeded
- unwinding is reverse; one failed compensation does not stop the others
- each compensation writes `begin` before it runs and `done` after, so a crash mid-undo is retried on resume and a
  finished one is skipped. Compensations must be idempotent; that is the contract.

---

## 6. The rules

1. **A cell is two crates and a manifest.** `crates/<name>-api` (types only) plus `crates/<name>` (the process) plus
   `cell.toml`. Nothing imports the process.
2. **One cell, one function.** If it covers two jobs, split it.
3. **Everything callable is a cell**, except a stateless library that composes (`cell-core`, `chain`).
4. **A capability is `In -> Out`, typed at both ends**, and addressed `api@major`. A caller states the major.
5. **A major is frozen when its successor ships.** Removal, retype, or a field becoming required gets a new major plus
   an **adapter cell** that speaks the old major and calls the new one.
6. **Every op declares the one right it needs.** The door denies by default; a capability never widens its own rights.
7. **State lives in `state.store`**, never in a file of the cell's own.
8. **No cell links another cell's code.** Import `-api` only.
9. **No MCP inside a cell.** The MCP is one cell that reaches the others through their capabilities.
10. **No scripts in a cell.** Rust only. A harness that needs a shell or a Python script lives in `tools/`.
11. **Names say what a thing does**, `<area>.<function>`, and **a cell ships with its docs**: what it does, the
    infrastructure it needs, its ops, its rights.

Rules 1 and 8 are the same rule seen from two sides: types cross, processes do not.

---

## 7. What a cell ships

Every cell repo contains, and its README states:

- what the cell does, in one sentence;
- the API it serves (`api@major`) and its ops, each with the one right it needs;
- the infrastructure it needs (`state.store@2`, a network, a device) and what happens when that is absent;
- how to run it (`cargo run`, `cell.toml` args) and how to test it;
- the refusal it is expected to give, with a test that watches it refuse.

A cell without those is not finished, however well it compiles.

---

## 8. The check

`check/` is the architecture rule, as a Rust binary that fails:

```bash
make check                                      # every cell under this repository
make check CELLS=path/to/cells                  # a cell tree elsewhere
cargo run --quiet --manifest-path check/Cargo.toml -- . --skip cells   # legacy subtree left out
```

Exit 0 when the graph is clean, 1 when it is not, naming the crate, the rule and the detail. It has no dependencies
beyond the Rust standard library, so it builds anywhere Rust builds and cannot drift with the crates it is checking.

It checks the shape, not the behaviour. Behaviour is `cargo test`; the two together are the gate.

---

## 9. The loop

The rule that makes many cells a week possible is the build, so it is measured, not assumed. These are measurements
from the reference workspace (`ce-net cells`, 27 Oct 2026), not targets for every machine:

| | before | now |
|---|---|---|
| cold check, one cell | 19.9 s | 24.1 s for **all 7 crates** |
| no-op check | — | **0.7 s** |
| edit + check, small crate (`cell-core`) | — | 5.2 s |
| edit + check, `chain` | — | 2.1 s |
| edit + check, `doc-store` | — | 13.2 s |
| full test suite (42 tests) | — | **34.7 s cold, 4.6 s warm** |

One workspace, one dependency graph, one shared target. A cell that is its own workspace pays the dependency build
again for every cell, and that is the cost that stops the practice.

---

## 10. Migrating what predates this

Three kinds of cells predate this document:

1. **a single crate whose API is a JSON string in a comment** (the oldest shape);
2. **a cell that is two crates but never declared a version** (half-converted);
3. **a cell that serves a major callers already use** (converted, but its shape is now frozen).

The order that does not break callers:

1. **Write the behaviour test first**, against what the cell does today. If there is no test, record the answers and
   prove they did not move. Converting a cell with no test is how a working thing stops working.
2. **Split it** into `-api` and process; move the documented shape into a `ce_wire::api!` block with numbered ops.
3. **Declare `cell.toml`** with `api`, `version`, `crate`, `args`, and `[launch] health`.
4. **Keep one `on::<Op>` per op**, calling the same functions it already had.
5. **For a major already in use, ship an adapter** (`version = 1`, `adapts = "<api>@2"`) before the old shape stops
   being served. Old callers keep working; the new major is built to this document.
6. `cargo test` with a `Server`/`Client` round trip; `cellcheck` goes MISS → ok.

`--skip` exists so the rule can be enforced on what is converted while the debt is visible on demand. The debt is not
a reason to weaken the rule; it is a list with an order.

---

## 11. Still open

1. **Where the active chain definition lives** — git (reviewable) or `state.store` (hot-reloadable). Proposal: git is
   the definition, `state.store` holds the signed active copy.
2. **Who holds a budget** — the room or the person. Proposal: the room, with the person billed through it.
3. **What `doc.ref` does when the machine holding the file is asleep** — fail honestly, or queue until it returns.
4. **Whether the chain library becomes codegen** so that even composition is a cell. Today it is a library, because a
   chain must be typed at compile time.

Each of these changes the shape of an API, which is why they are written down rather than decided by whoever is awake.

---

## 12. Why this is the default, and what changed

On 27 Oct 2026 the owner made this the default for **all new and converted cells in ce-net**, not only for the tree it
was written in:

> "Post this so that this is the default on all of ce-net with the architecture and not just here locally... default
> for all new and converted ce-net cells and we use adapter cells and adapter architecture for the rest."

Two consequences follow, and both are in this document already:

- **New and converted cells follow this document.** The check in §8 runs on them, and `make cells` fails when a rule
  is broken.
- **The rest are not rewritten at once.** They keep answering through adapters (§4, §10). "Adapter architecture" is
  not a weaker rule; it is the rule applied to a cell whose callers already exist.

The record of the announcement, the discussion, and the posts this document links to on the Trana graph is in
[NETWORK.md](NETWORK.md).
