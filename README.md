# BusinessCentral.AL.Runner

[![Test Matrix](https://github.com/StefanMaron/BusinessCentral.AL.Runner/actions/workflows/test-matrix.yml/badge.svg)](https://github.com/StefanMaron/BusinessCentral.AL.Runner/actions/workflows/test-matrix.yml)
[![NuGet](https://img.shields.io/nuget/v/MSDyn365BC.AL.Runner)](https://www.nuget.org/packages/MSDyn365BC.AL.Runner)

Run Business Central AL unit tests in **milliseconds** — no BC service tier, no Docker, no SQL Server, no license required.

## What It Is

AL Runner is a standalone test executor for Business Central codeunits. It transpiles AL source to C# using the BC compiler's public API, rewrites the generated C# to replace BC runtime types with in-memory mocks, compiles everything with Roslyn, and executes your test codeunits directly.

```
AL Source
  ↓  BC Compilation.Emit()
Generated C#
  ↓  RoslynRewriter (BC types → mocks)
Rewritten C#
  ↓  Roslyn in-memory compile
.NET Assembly
  ↓  Test discovery + execution
Results in milliseconds
```

It works well for pure-logic codeunits. For the remaining gaps, see [What's Missing](#whats-missing) below.

## Why

Running a full BC CI pipeline (compile, publish, initialize, run tests) takes 45+ minutes. AL Runner makes the pure-logic unit test portion take under a second, giving you a fast inner loop for codeunit logic that doesn't depend on UI, HTTP, or external services.

AL Runner is designed to run **before** the full BC service tier pipeline as a fast pre-check. It does not replace the full pipeline.

## What It Supports

**Supported:**
- Codeunit logic (fields, variables, arithmetic, string ops, enums/options)
- In-memory record store: Init, Insert, Modify, Get, Delete, DeleteAll, FindFirst, FindLast, FindSet, Next
- Composite primary keys, sort ordering (SetCurrentKey/SetAscending)
- SETRANGE and SETFILTER filtering (=, <>, <, <=, >, >=, wildcards, OR separators)
- Cross-codeunit dispatch via MockCodeunitHandle
- Assert codeunit (ID 130): AreEqual, AreNotEqual, IsTrue, IsFalse, ExpectedError, RecordIsEmpty, etc.
- `asserterror` keyword (catches expected errors) + `GetLastErrorText()`
- `Error()` / `Message()` — Error throws an exception; Message writes to console
- OnValidate triggers on table fields
- Table procedures (custom procedures on table objects)
- IsolatedStorage (in-memory key-value store)
- TextBuilder (in-memory string builder)
- Format/Evaluate type conversions
- AL interfaces injected by test code
- AL arrays (MockArray, MockRecordArray)
- AL Variant (MockVariant)
- RecordRef / FieldRef (Open, Close, Field(n).Value get/set, Insert, Modify, Delete, DeleteAll, FindSet, Next, GetTable, SetTable, SetRange, SetFilter, RecordId)
- Built-in session functions: CompanyName, UserId, TenantId, SerialNumber (return empty string)
- Input from .al files, directories, or .app packages
- Partial compilation (skips unsupported object types like XMLport)
- Stub files (`--stubs <dir>`) for replacing unsupported dependencies
- Stub generation (`--generate-stubs`) from .app symbol packages
- Statement-level coverage reporting (`--coverage`, outputs cobertura.xml)
- Per-iteration loop tracking (`--iteration-tracking`)
- Machine-readable JSON output (`--output-json`)

**Not supported (by design):**
- Page, Report, XMLPort — inject via AL interface or exclude from runner
- HTTP requests — inject via AL interface or exclude from runner
- Event subscribers — implicit events (OnAfterModify, OnAfterInsert, etc.) do NOT fire
- .app file loading as test input (source directories only; .app supported for symbol references)
- Filter groups (FilterGroup)

## What It Doesn't Support (and Why That's OK)

The runner has a deliberate scope boundary: **if you can't inject a dependency via an AL interface, that code path isn't unit-testable in standalone mode**.

This is a design decision, not a bug. Code that truly depends on the BC service tier (page actions, HTTP calls, events fired by the DB tier) should be tested in the full pipeline. The runner covers the logic layer.

## Pipeline Outcomes

When al-runner executes a test codeunit, exactly one of three things happens:

**1. FAIL — test failure caught**
An assertion failed or the test threw an exception. This is a real failure. Pipeline stops immediately. If al-runner says FAIL, it is a real failure.

**2. ERROR — runner cannot execute the codeunit**
The codeunit depends on an unsupported feature and crashes. This is a configuration error, not a test failure. Fix it by either removing that codeunit from the runner config, or injecting the missing dependency via an AL interface.

**3. PASS**
The codeunit's direct logic is correct. Note: if the test implicitly depends on an event subscriber (e.g., `OnAfterModify` fires a trigger that modifies state the test then asserts), the runner will PASS silently because implicit events don't fire. The full BC service tier pipeline runs after the runner and catches these cases.

**The guarantee:** if al-runner says FAIL, it is a real failure. Silent passes due to missing event subscribers are an accepted known limitation — always run the full pipeline after al-runner.

## What's Missing

Known gaps for real-world use:

1. **Implicit event publishers on DB operations** — `OnAfterModify`, `OnAfterInsert`, etc. do NOT fire. Tests that depend on event subscribers will produce silent false positives (see test 05).
2. **Page, Report, XMLPort** — not supported. Inject via AL interface, use `--stubs`, or exclude from runner.
3. **HTTP** — not supported. Inject via AL interface.
4. **Filter groups** (FilterGroup) — not tracked.
5. **ALGetFilter** — returns empty string even when filters are active.
6. **BLOB / InStream / OutStream** — not supported.

## Developer Contract

Design your codeunits for testability by injecting dependencies via AL interfaces:

```al
// Define the interface
interface IInventoryCheck
    procedure HasStock(ItemNo: Code[20]): Boolean;
end

// Inject it into the codeunit
codeunit 50100 OrderProcessor
    procedure Process(ItemNo: Code[20]; Checker: Interface IInventoryCheck)
    begin
        if not Checker.HasStock(ItemNo) then
            Error('Item %1 is out of stock', ItemNo);
        // ... rest of logic
    end;
end

// In your test codeunit:
// Implement IInventoryCheck with a stub that always returns true/false
```

Anything you can't inject cannot be unit-tested by this runner — and that's the right boundary.

## Quick Start

### Install

```bash
dotnet tool install --global MSDyn365BC.AL.Runner
```

That's it. On first build/run, the AL compiler (~57 MB from NuGet) and BC Service Tier DLLs (~11 MB via HTTP range requests) are downloaded automatically and cached. No manual setup, works on Windows, Linux, and macOS.

### Run

```bash
# Run test codeunits (test mode auto-detected when Subtype = Test is present)
al-runner ./src ./test

# Run with coverage report
al-runner --coverage ./src ./test

# Load from .app packages with dependency resolution
al-runner --packages ./packages MyApp.app MyApp.Tests.app

# Provide stub AL files for unsupported dependencies
al-runner --stubs ./stubs ./src ./test

# Verbose output (show transpilation/compilation details)
al-runner -v ./src ./test

# Machine-readable JSON output
al-runner --output-json ./src ./test

# Run a single test procedure by name
al-runner --run TestMyProcedure ./src ./test

# Track per-iteration loop data (requires --output-json)
al-runner --iteration-tracking --output-json ./src ./test

# Capture variable values after each test for inline display
al-runner --capture-values ./src ./test

# Generate stub AL files from .app symbol packages
al-runner --generate-stubs .alpackages ./stubs
al-runner --generate-stubs .alpackages ./stubs ./src ./test  # only referenced codeunits

# Run inline AL code
al-runner -e 'codeunit 99 X { trigger OnRun() begin Message('"'"'hi'"'"'); end; }'

# Print test-writing guide for AI agents
al-runner --guide

# Debug: dump generated C# before and after rewriting
al-runner --dump-csharp ./src
al-runner --dump-rewritten ./src
```

### Build from source

```bash
dotnet build AlRunner/
dotnet run --project AlRunner -- ./src ./test
```

### Config file (al-runner.json)

```json
{
  "sourcePath": "./src",
  "testPath": "./test",
  "testCodeunits": [50200, 50201]
}
```

Config-based invocation is not yet wired into the CLI (future work).

## How It Fits in the Full Pipeline

AL Runner is designed to sit before the full BC service tier in CI:

```
Pull Request
  ↓
al-runner (seconds) — catches pure-logic failures fast
  ↓ (only if al-runner passes)
Full BC pipeline (MsDyn365Bc.On.Linux, 45+ min) — full fidelity test execution
```

The full BC service tier pipeline:
- https://github.com/StefanMaron/MsDyn365Bc.On.Linux

## How It Works

AL Runner has a 4-stage pipeline:

```
AL source (.al files)
  ↓  BC Compilation.Emit()        Transpiles AL to C# using the BC compiler's public API
  ↓  RoslynRewriter               Rewrites BC runtime types to in-memory mocks (AST-level)
  ↓  Roslyn in-memory compile     Compiles the rewritten C# against BC Service Tier DLLs
  ↓  Executor                     Discovers [NavTest] methods, runs them, reports results
```

**Stage 1 — AL Transpiler**: Uses `Microsoft.Dynamics.Nav.CodeAnalysis.Compilation.Emit()` to convert each AL object (table, codeunit) into a C# class. The AL compiler is downloaded from NuGet automatically on first build.

**Stage 2 — RoslynRewriter**: A `CSharpSyntaxRewriter` that transforms the generated C# for standalone execution. Replaces `NavRecordHandle` → `MockRecordHandle`, `NavCodeunitHandle` → `MockCodeunitHandle`, strips BC attributes, rewrites `NavDialog.ALMessage` → `AlDialog.Message`, etc. This is where the BC runtime dependency is severed.

**Stage 3 — RoslynCompiler**: Compiles the rewritten C# in-memory with Roslyn. References the BC Service Tier DLLs (auto-downloaded from the BC artifact CDN via HTTP range requests — ~11 MB instead of the full 1.2 GB artifact). No files written to disk.

**Stage 4 — Executor**: Discovers test codeunits (classes with `[NavTest]` methods), resets the in-memory table store between tests, invokes each test via reflection, and reports pass/fail/error with coverage.

All dependencies are auto-downloaded and cached. The only prerequisite is .NET 8 SDK.

## Test Cases

The `tests/` directory contains 70 test cases. Each is a self-contained AL project (`src/` + `test/`) that exercises a specific runner capability. Every push runs all test cases against a [matrix of BC versions](https://github.com/StefanMaron/BusinessCentral.AL.Runner/actions/workflows/test-matrix.yml) (26.0 through 27.5).

| Test case | What it covers |
|---|---|
| `01-pure-function` | Pure calculation logic, Assert.AreEqual |
| `02-record-operations` | Record CRUD, SETRANGE filtering |
| `03-interface-injection` | AL interface for dependency injection |
| `04-asserterror` | Error validation with asserterror + Assert.ExpectedError |
| `05-known-limitation` | Silent false positive from missing event subscriber |
| `06-intentional-failure` | Deliberately broken tests for error output demo |
| `07-composite-pk` | Composite (multi-field) primary keys |
| `08-sort-ordering` | SetCurrentKey / SetAscending sort ordering |
| `09-setfilter-expressions` | Complex SETFILTER expressions (wildcards, OR) |
| `10-cross-codeunit` | Cross-codeunit dispatch via Codeunit.Run |
| `11-variant-type` | AL Variant type boxing/unboxing |
| `12-format-string` | Format() and Evaluate() type conversions |
| `13-partial-compile` | Partial compilation (skips unsupported object types) |
| `14-assert-130000` | Assert codeunit with ID 130000 (alternate ID) |
| `15-codeunit-assign` | Codeunit variable assignment |
| `16-isolated-storage` | IsolatedStorage key-value operations |
| `17-text-builder` | TextBuilder Append/AppendLine/ToText |
| `18-validate-trigger` | OnValidate triggers on table fields |
| `19-table-procedures` | Custom procedures on table objects |
| `20-option-fields` | Option/Enum fields on tables |
| `21-expected-error-substring` | ExpectedError substring matching |
| `22-record-persistence` | Record persistence across procedure calls |
| `23-error-line-mapping` | Error line mapping in test output |
| `24-secret-text` | SecretText type handling |
| `25-expected-error-code` | ExpectedErrorCode assertion |
| `26-time-format` | Time formatting |
| `27-testfield-error` | TestField error messages |
| `28-table-extension-fields` | Table extension field support |
| `29-record-id` | ALRecordId support |
| `30-modify-all` | ModifyAll on filtered records |
| `31-interface-return` | Interface return from functions |
| `32-interface-param` | Interface parameter passing |
| `33-extension-validate` | OnValidate in table extensions |
| `34-extension-parent-object` | Extension parent object access |
| `35-validate-no-value` | Validate without explicit value |
| `36-page-ext-no-cascade` | Page extension no-cascade behavior |
| `37-event-scope` | Event subscriber scoping |
| `38-page-ext-currpage` | Page extension CurrPage access |
| `39-stubs` | Stub file loading |
| `40-page-run-record` | Page.Run with record parameter |
| `41-try-function` | TryFunction error handling |
| `42-list-of-interface` | List of interface type |
| `43-recordref-local` | RecordRef local variable |
| `44-single-arg-validate` | Single-argument Validate |
| `45-unknown-namespace-using` | Unknown namespace using directive |
| `46-missing-dep-hint` | Missing dependency hint in errors |
| `47-capture-inline` | Inline value capture |
| `48-page-variable` | Page variable handling |
| `49-recordref-open` | RecordRef.Open |
| `50-enum-ordinals` | Enum ordinal values |
| `51-init-value` | Field InitValue properties |
| `52-setfilter-and` | SetFilter with AND conditions |
| `53-enum-interface` | Enum implements interface |
| `54-numbersequence` | NumberSequence support |
| `55-flowfield-exist` | FlowField existence |
| `56-flowfield-multi` | FlowField with multiple tables |
| `57-navapp-moduleinfo` | NavApp.GetModuleInfo |
| `58-hyperlink` | Hyperlink function |
| `59-dateformula-validate` | DateFormula validation |
| `60-guid-text-get` | GUID to Text conversion with Get |
| `61-enum-names` | Enum.Names() support |
| `62-pk-unique` | Primary key uniqueness on Insert |
| `63-oninsert-trigger` | OnInsert trigger firing |
| `64-recref-inmem` | RecordRef backed by in-memory store |
| `65-page-helper` | Page helper dispatch |
| `66-event-subscribers` | Event subscriber binding |
| `67-iteration-tracking` | Per-iteration loop tracking |
| `68-strsubstno-integer` | StrSubstNo with integer arguments |
| `69-recref-fieldref` | RecordRef/FieldRef full runtime |
| `70-companyname` | CompanyName/UserId/TenantId/SerialNumber |

When a new scenario is encountered that should work but doesn't, it gets triaged:
- **In scope** → add a test case, fix the runner, verify it passes across all BC versions
- **Out of scope** → document as a known limitation (like test 05)

To add a test case: create `tests/NN-name/src/*.al` and `tests/NN-name/test/*.al`. The CI workflow auto-discovers all test directories (except `06-intentional-failure`).

## CI

The [Test Matrix](https://github.com/StefanMaron/BusinessCentral.AL.Runner/actions/workflows/test-matrix.yml) runs on every push — resolving the latest patch version for each BC major.minor, building and testing in parallel. The [Publish](https://github.com/StefanMaron/BusinessCentral.AL.Runner/actions/workflows/publish.yml) workflow pushes to NuGet only when all versions pass (triggered by `git tag v*`).

## Naming

Follows the `BusinessCentral.AL.*` convention:
- [BusinessCentral.AL.Mutations](https://github.com/StefanMaron/BusinessCentral.AL.Mutations) — mutation testing
- BusinessCentral.AL.Runner — this repo

## License

MIT
