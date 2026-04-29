# AL.Runner Protocol v2 — Upstream PR Rationale

**Branch:** `feat/alchemist-protocol-v1` (fork at SShadowS/BusinessCentral.AL.Runner)
**Scope:** 20 commits, ~5300 lines, 255 tests pass, 6 pre-existing failures unchanged.
**Audience:** upstream maintainers reviewing PRs split out of this branch.

---

## The Problem That Drove It

ALchemist v0.4.0 surfaced two correctness gaps + several UX gaps that all rooted in the AL.Runner `--server` runtests response shape:

1. **Inline error decoration at wrong line.** AL.Runner stripped `alSourceLine`/`alSourceColumn` from `runtests` responses even though it had them. For runtime errors (not assertions) the deepest user frame was buried in a text `stackTrace` blob — no structured way to surface "the .al line where the error actually happened."
2. **Coverage gutter icons missing.** `runtests` had no coverage data. CLI mode's `--coverage` writes `cobertura.xml` to disk; server mode skipped coverage entirely.
3. **`runtests` sparse vs `execute`.** Per-test `Message()` output, `capturedValues`, iteration data — all silently dropped during `runtests` serialization but emitted by `execute`.
4. **Server runs everything.** No way to narrow a run to a single test or codeunit.
5. **No live updates.** Test Explorer waited for the entire run; stale UI for slow suites.
6. **No mid-run cancel.** Stop button wired to nothing useful.

---

## The Foundational Primitive: `#line` Directives

Single most impactful change. Spec argument: once Roslyn writes `.al` filenames into IL pdb sequence points, every other feature falls into place using **standard .NET machinery** — no custom runtime walking, no custom attribute scrapers, future debugger work uses the same data.

**Why this is the right primitive:**
- `StackFrame.GetFileName()` returns `.al` paths natively.
- Coverage tools read pdb sequence points without modification.
- One source of truth — eliminates the parallel `AlScope.LastStatementHit` shadow-tracker drift.
- A future Debug Adapter Protocol implementation drops in cleanly.

**Why a fork branch first:** the BC transpiler is upstream/external. We can't change the AL→C# emitter. The cleanest insertion point is **post-rewrite syntax-tree manipulation**: walk the rewritten C# tree, find `StmtHit(N)` / `CStmtHit(N)` markers (which already encode `(scope, stmtIdx)`), look up `(scope, stmtIdx) → AL line` from `SourceLineMapper._sourceSpans`, prepend `#line N "src/Foo.al"` as leading trivia. No transpiler change. Plus: gate Roslyn at `OptimizationLevel.Debug` and emit portable PDB so `Assembly.Load(asmBytes, pdbBytes)` makes stack traces honor the directives.

---

## The 12-Task Implementation Defense

| # | Change | Defense |
|---|---|---|
| 1 | `protocol-v2.schema.json` | Single source of truth across two repos. ALchemist's tests validate emitted lines against it; drift caught immediately. |
| 2 | `AlStackFrame`, `AlErrorKind`, `FramePresentationHint`, `TestFilter` types | Pure value types; foundation for everything that follows. |
| 3 | `StackFrameMapper` | Reads standard managed `Exception.StackTrace` text. Classifies frames: `.al` filename → user code (Normal); `AlRunner.Runtime.*` / `Microsoft.Dynamics.*` → Subtle. Returns DAP-shaped frames. **Why suffix-prefixed predicates and not bare `Mock*`:** initial review caught false positives on user codeunits named `MockAccountTest`. The narrow predicate set (`AlRunner.Runtime.` + `Microsoft.Dynamics.`) is the canonical truth. |
| 4 | `ErrorClassifier` | Heuristics on exception type-name suffix — assertion / runtime / compile / setup / timeout / unknown. Drives IDE UI variation. **Setup is real:** T8 wires `insideTestProc` flag flipped just before `OnRun.Invoke`, so exceptions during `InitializeComponent` correctly classify as setup. |
| 5 | `CoverageReport.ToJson` | Mirrors existing `WriteCobertura` resolution loop, producing structured per-file `FileCoverage[]` for inline emission. **Sums hits, not max-1:** matches the `Plan A → Plan B+D` IDE rendering goal where multi-statement lines deserve detail. Cobertura keeps clamp-to-1 for external-tool compatibility. Aggregator extracted, both consumers share the resolution logic. |
| 6 | `LineDirectiveInjector` | **The foundational change.** Trivia-injected `#line N "path.al"` before every `StmtHit`/`CStmtHit`-anchored statement. Plus `OptimizationLevel.Debug` (PDB sequence points need it) and portable-PDB emit. Gated by `EmitLineDirectives` flag — CLI users opt in. Reviewer caught: re-entry guard skips injection if statement already has a `LineDirectiveTriviaSyntax`; tests cover spaces in paths, `CStmtHit` if-statement branch, line-number correctness against fixture. |
| 7 | `Executor.RunTests` revised | Adds `TestFilter`, `onTestComplete`, `CancellationToken`. **Per-test isolation via `AsyncLocal<TestExecutionState>`:** `MessageCapture`/`ValueCapture` dual-write — always to the per-test scope (so `TestEvent.messages`/`capturedValues` work) AND to the global aggregate when `Enable()` is set (so existing pipeline-level capture keeps working). `BuildErrorResult` extracted across the 5 catch branches — single source of truth for failure-result construction. |
| 8 | `Server.cs cancel` command | New JSON-RPC command. Sets `_activeRequestCts`. Acks `{type:"ack",command:"cancel",noop:bool}`. **Concurrent dispatch** so cancel arrives mid-run: dispatch loop reads stdin while `runtests` streams; only `cancel` is permitted as side-channel. `_outputLock` semaphore serializes writes between the dispatch loop and the runtests worker. Wins: true cooperative mid-run cancel without breaking stdin protocol simplicity. |
| 9 | `Server.cs` NDJSON streaming + protocol v2 | One `{type:"test"}` line per test as it completes; terminal `{type:"summary",protocolVersion:2}`. Field parity with execute response (`alSourceLine`, `errorKind`, `stackFrames` DAP-shaped, `messages`, `capturedValues`, structured `coverage`). Forward-compat: schema permits unknown fields; v1 clients fall back via summary `protocolVersion` absence. |
| 10 | Cache extension | `CompilationCache` carries `SourceSpans`, `ScopeToObject`, `TotalStatements`, `SourceFileMapper`/`SourceLineMapper` snapshots. **Why all of them:** cache hits previously bypassed the rewrite/compile pipeline, so coverage couldn't be emitted on second-run-of-same-project. The static singletons (`SourceFileMapper`, `SourceLineMapper`) are written by Pipeline only on cache-miss; on cache-hit, multi-slot LRU rotation A→B→A would leave them stale. Snapshot+restore makes correctness independent of hit/miss. |
| 11 | Schema validation test | Real-binary smoke output validates against `protocol-v2.schema.json`. Catches drift between emitter and contract immediately. |
| 12 | E2E smoke against built Release binary | Recorded in `docs/protocol-v2-samples/runtests-coverage-success.ndjson`. ALchemist consumes the same shape in unit tests without spawning a runner. |

---

## The Late Fix (`f2d2bb3`)

Discovered after Plan E2 ALchemist consumer shipped: **passing tests didn't get `alSourceFile`**. The original spec defined `alSourceFile` as "deepest user frame from stack walk" — only populated on failures (where there's an exception to walk). Passing tests had `alSourceFile: undefined`, breaking ALchemist's inline-decoration file-filter.

Two fixes in one commit:

1. **Per-test `alSourceFile` fallback.** When stack-walk doesn't fire (passing test), populate `t.AlSourceFile` from `SourceFileMapper.GetFile(t.CodeunitName)` so the test event always carries a file context. Cheap because the runner already has this mapping for cobertura emission.
2. **Per-capture `alSourceFile` (the load-bearing one).** Each captured-value record now carries `alSourceFile` resolved from its own `objectName`. Captures from a codeunit invoked indirectly by the test (e.g. `TestCU` calls into `CU1`) correctly attribute to `CU1.al`, not the test's file — matching where the assignment actually happened in the user's editor.

This is genuinely a runner concern, not consumer-side: only the runner has the authoritative `objectName → file` mapping (`SourceFileMapper`). ALchemist would otherwise need to duplicate that map.

---

## Defensible Architectural Invariants

1. **`#line` directives are the primitive.** Every other feature builds on standard .NET machinery (`StackFrame.GetFileName`, pdb sequence points, native debugger attach). No custom runtime walking, no parallel attribute scraping.
2. **NDJSON streaming.** One request → multiple response lines (`type` discriminator). Forward-compat to future event types.
3. **Protocol versioning.** Summary's `protocolVersion: 2` is the single discriminator. v1 clients silently fall back without breakage.
4. **Backward compat preserved.** Existing CLI users (cobertura.xml writers, `--dump-csharp`, `--run-procedure`) unchanged. The 6 pre-existing test failures are summary-line-format-string-related, predate this branch, untouched.

---

## Suggested Upstream PR Split (9 PRs)

From the Plan E1 final review:

1. **Protocol surface** — `protocol-v2.schema.json` + types (`AlStackFrame`, `AlErrorKind`, `FramePresentationHint`, `TestFilter`). Pure additions.
2. **`StackFrameMapper`** — exception → structured DAP-aligned frames.
3. **`ErrorClassifier`** — exception → error-kind enum (assertion/runtime/compile/setup/timeout/unknown).
4. **`CoverageReport.ToJson`** + Cobertura aggregator extraction. Both consumers share resolution logic.
5. **`#line` directive injector + portable PDB** (the perf-hazard one — gated by `EmitLineDirectives` flag, CLI opt-in).
6. **`Executor.RunTests` revised** — `TestFilter` + `onTestComplete` + `CancellationToken` + AsyncLocal per-test isolation.
7. **Server `cancel` command** — new JSON-RPC command + `_activeRequestCts` lifecycle.
8. **Server NDJSON streaming + protocol v2** (the capstone) — concurrent dispatch, field-parity test event, terminal summary with `protocolVersion: 2`. Cache extension lands here.
9. **Per-capture `alSourceFile`** (`f2d2bb3`) — depends on PR 8.

---

## Bottom-Line Defense

Every change addresses a concrete shipped-product gap in ALchemist v0.4.0 that the AL.Runner protocol shape made unfixable downstream. The architectural choice (post-rewrite trivia injection rather than transpiler change) keeps us out of the BC compiler. The cache-singleton snapshot work and the AsyncLocal per-test isolation are correctness fixes that benefit any future server consumer, not just ALchemist.

The fork-first development model meant we could iterate end-to-end against a real workload (Sentinel + ALchemist) before splitting into reviewable upstream PRs — surfacing real bugs (e.g. the per-capture `alSourceFile` gap) that a paper-only spec review would have missed.
