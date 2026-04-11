# Changelog

All notable changes to this project are documented here. Format based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), versioning follows
[SemVer](https://semver.org/spec/v2.0.0.html).

## [1.0.4] — 2026-04-11

### Added
- `NavApp.GetModuleInfo` / `GetCurrentModuleInfo` / `GetCallerModuleInfo`
  routed through a new `MockNavApp` stub. The real `ALNavApp` loads
  `Microsoft.Dynamics.Nav.CodeAnalysis` (not shipped with al-runner),
  so any code path that reached NavApp metadata crashed with an
  assembly-load failure. The stub returns `false` for every lookup
  and leaves the ByRef `ModuleInfo` untouched, matching BC's
  "not found" contract. ([#22](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/22))

### Fixed
- **Multi-field FlowField `exist(...)`** now works. 1.0.3 covered the
  single-field case but the multi-condition variant
  (`exist(Child where(C1 = field(X), C2 = field(Y)))`) silently
  returned false: the `CalcFormulaRegistry` regex was non-greedy and
  stopped at the first `)` — the one closing `field(X)` — so the
  second clause was lost. Parser now paren-walks the `exist(...)`
  body manually, splits top-level commas in the `where(...)` body,
  and resolves child field IDs through a new transpile-time
  `TableFieldRegistry` (previously relied on runtime
  `RegisterFieldName`, which only fired when generated code referenced
  `ALFieldNo(name)` explicitly).
  ([#15](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/15) follow-up)

## [1.0.3] — 2026-04-11

### Added
- `CHANGELOG.md` shipped inside the NuGet package; `<PackageReleaseNotes>`
  points nuget.org at it.
- Publish workflow now creates a GitHub Release on tag push, seeded with
  the matching `CHANGELOG.md` section and the `.nupkg` attached.
- Missing-dependency diagnostic now enriches with a namespace-mismatch
  hint when a stub with the matching type+name was loaded under a
  different namespace. ([#9](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/9))
- Server mode: multi-slot LRU cache (8 slots) keyed by a per-file
  fingerprint, and the `runTests` response now includes a `changedFiles`
  array on cache miss so IDE integrations can show change-aware
  feedback. Bouncing between projects in one session no longer
  invalidates the previous entry.
  ([#10](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/10) — MVP; full dep-graph partial recompile still open)
- **Per-statement value capture**: `--capture-values` now emits a
  Quokka-style timeline of intermediate values, not just a
  final-state snapshot. A new `ValueCaptureInjector` pass injects
  `ValueCapture.Capture(...)` after each scope-field assignment,
  keyed by the neighboring `StmtHit(N)` so captures map back to AL
  source lines. Post-test reflection-based capture is kept as a
  fallback for variables the injector can't reach.
  ([#11](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/11))
- **Server `execute` command**: new JSON-RPC command that accepts
  either inline AL (`code`) or `sourcePaths` and runs the first
  codeunit's `OnRun` trigger in run-mode. Response mirrors
  `runTests` plus captured `messages` and optional `capturedValues`.
  ([#12](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/12))
- **Column precision in error mapping**: `TestResult` and
  `--output-json` now include `alSourceColumn` alongside
  `alSourceLine`. `FormatDiagnostic` emits `[AL line ~N col M in X]`.
  The existing `CoverageReport.ParseSourceSpans` encoding already
  carried columns; they were discarded.
  ([#13](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/13))
- **`Enum::X.Ordinals()` / `.Names()`** resolve against a transpile-time
  `EnumRegistry` built from the AL source. BC inlines enums so runtime
  reflection can't recover the member list. ([#17](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/17))
- **Enum-implements-interface dispatch** (`Flag := Strategy;`). BC stores
  the NavOption directly in the interface handle; `MockInterfaceHandle`
  now intercepts it, looks up the per-value
  `Implementation = "Iface" = "Codeunit"` mapping in `EnumRegistry`,
  and resolves the codeunit through the new `CodeunitNameRegistry`.
  ([#20](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/20))
- **Table `InitValue` defaults** applied by `Rec.Init()` via a new
  `TableInitValueRegistry` — supports Boolean, Integer, Decimal, Text
  and Enum member init values.
  ([#18](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/18))
- **FlowField `exist()` `CalcFields`** evaluated against in-memory
  tables via a new `CalcFormulaRegistry`. Supports
  `where(field = field(...))` and `where(field = const(...))`
  conditions; `count` / `sum` / `lookup` still return defaults.
  ([#15](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/15))
- **`NumberSequence`** replaced with a process-local
  `MockNumberSequence` keyed by name. `Exists` / `Insert` / `Next` /
  `Current` / `Restart` no longer throw `NullReferenceException` via
  `NavSession`. ([#14](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/14))
- **`Page "X"` local variables** transpile to a `MockFormHandle`
  stub (like the existing `MockInterfaceHandle` / `MockRecordRef`).
  ([#21](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/21))

### Fixed
- **`SetFilter` AND operator (`&`)** — AL filter expressions with AND
  chains were silently OR-ed, matching too many rows.
  `MatchesFilterExpression` now splits on `|` (OR) first, then on
  `&` (AND) inside each alternative, matching BC's precedence.
  Wildcards, `..` ranges, `@` case-insensitive, and per-field
  AND-across-fields all still work. `%1..%n` placeholder substitution
  covered for integer and text values, including inside mixed AND/OR
  precedence expressions.
  ([#19](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/19))
- **`Page.Run(Page::X, Rec)` / `Page.RunModal`** with fully-qualified
  `NavForm` method access, and `Page "X"` local variable initialisation
  via `NavFormHandle` — both no longer cascade-exclude the containing
  codeunit. (Follow-up to #6, with a real repro via #21.)
- **`RecordRef` 3-arg `Open(tableId, temporary, company)`** now has
  matching `ALOpen(CompilationTarget, int, bool, string)` overloads,
  and `ALIsEmpty` is exposed as a property to match BC's lowering of
  `!recRef.IsEmpty`.
  ([#16](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/16))
- `AL0791 namespace unknown` on an unused `using` directive no longer
  blocks compilation; added to the ignored-error set alongside
  `AL0432` / `AL0433`. Genuine unresolved uses still surface as
  separate errors. ([#8](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/8))
- Regression test for single-arg `Record.Validate("Field")` covering
  Decimal, DateFormula, and error propagation paths. The underlying
  2-arg `ALValidateSafe` overload was added before the report was
  filed; this commit just locks the behavior in.
  ([#7](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/7))

### Internal
- `Pipeline.Run` now redirects both `Console.Out` and `Console.Error`
  into the captured `StringWriter` instances for the duration of the
  run, so `AlDialog.Message` and `PrintResults` no longer corrupt
  the server's stdin/stdout JSON protocol.

## [1.0.2] — 2026-04-11

### Fixed
- `Page.RunModal(PageId, Rec)` as a bare statement no longer emits
  invalid C# (`default(FormResult);`). Strips `NavForm.Run/RunModal/SetRecord`
  at statement level. ([#6](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/6))
- `[TryFunction]`-attributed procedures now compile and run: `AlScope`
  gains `TryInvoke(Action)` / `TryInvoke<T>(Func<T>)` overloads that
  execute the delegate, catch any exception, and return true/false.
  ([#4](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/4))
- `List of [Interface X]` no longer cascades-excludes the containing
  object. New `MockObjectList<T>` replaces BC's `NavObjectList<T>`
  (which requires `T : ITreeObject` and a non-null Tree handler),
  and `ALCompiler.ToInterface(this, x)` is rewritten to
  `MockInterfaceHandle.Wrap(x)`.
  ([#3](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/3))
- Declaring `var RecRef: RecordRef` no longer cascades-excludes the
  containing codeunit. `NavRecordRef` is rewritten to a new
  parameterless `MockRecordRef` stub with no-op Open/Close/IsEmpty/
  Find/Next/Count. Consistent with the documented policy that
  RecordRef/FieldRef compile but do not function at runtime.
  ([#5](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/5))
- `AL0791 namespace unknown` on an unused `using` directive no longer
  blocks compilation; added to the ignored-error set alongside
  `AL0432` / `AL0433`. Genuine unresolved uses still surface as
  separate errors. ([#8](https://github.com/StefanMaron/BusinessCentral.AL.Runner/issues/8))

### CI
- Publish workflow now mirrors the test matrix: runs the C# test
  project and excludes `tests/39-stubs/` from the bulk run, invoking
  it separately with `--stubs`. Builds `AlRunner.slnx` so the test
  DLL exists by the time `dotnet test --no-build` runs.

## [1.0.1] — 2026-04-10

### Changed
- Per-suite test invocation restored (single-invocation run had ID
  conflicts); test timings back to ~75 s total but reliable.

## [1.0.0] — 2026-04-10

### Added
- `--output-json` machine-readable test output.
- `--server` long-running JSON-RPC daemon over stdin/stdout.
- `--capture-values` variable-value capture for Quokka-style inline
  display.
- `--run <ProcedureName>` single-procedure execution.
- Error line mapping via last-statement tracking.
- C# test infrastructure (`AlRunner.Tests/`) covering pipeline,
  server, capture-values, single-procedure, error mapping and
  incremental server-mode caching.

### Changed
- All BC versions 26.0 → 27.5 now run on every push via the test
  matrix workflow.

## [0.2.0] — 2026-04-10

### Added
- `--coverage` Cobertura XML output wired into CI job summaries.
- NuGet package ID standardized to `MSDyn365BC.AL.Runner`.

## [0.1.0] — 2026-04-10

Initial release — AL transpile + Roslyn rewriter + in-memory execution
for pure-logic codeunits. No BC service tier, no Docker, no SQL, no
license. Test runner with `Subtype = Test` discovery and `Assert`
codeunit mock.

[1.0.4]: https://github.com/StefanMaron/BusinessCentral.AL.Runner/releases/tag/v1.0.4
[1.0.3]: https://github.com/StefanMaron/BusinessCentral.AL.Runner/releases/tag/v1.0.3
[1.0.2]: https://github.com/StefanMaron/BusinessCentral.AL.Runner/releases/tag/v1.0.2
[1.0.1]: https://github.com/StefanMaron/BusinessCentral.AL.Runner/releases/tag/v1.0.1
[1.0.0]: https://github.com/StefanMaron/BusinessCentral.AL.Runner/releases/tag/v1.0.0
[0.2.0]: https://github.com/StefanMaron/BusinessCentral.AL.Runner/releases/tag/v0.2.0
[0.1.0]: https://github.com/StefanMaron/BusinessCentral.AL.Runner/releases/tag/v0.1.0
