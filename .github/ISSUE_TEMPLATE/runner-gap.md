---
name: Runner gap / missing mock
about: A BC AL feature, type, or method that al-runner doesn't support yet
title: ''
labels: ''
assignees: ''
---

## Problem

<!-- What AL code triggers the issue? Paste the error message or describe the failure. -->

```
<!-- Compilation error or runtime exception here -->
```

**Triggered by:** <!-- e.g. "parallel-worker-bc", "my project", inline AL -->

## Reproduction

<!-- Minimal AL code that causes the error. Include both source and test codeunit. -->

```al
// Source codeunit
codeunit 50100 MyCodeunit
{
    procedure DoSomething()
    begin
        // AL that triggers the gap
    end;
}
```

```al
// Test codeunit
codeunit 50101 MyCodeunitTest
{
    Subtype = Test;
    var Assert: Codeunit Assert;

    [Test]
    procedure TestDoSomething()
    begin
        // test that fails or errors
    end;
}
```

## Root cause

<!-- What BC runtime type or method is missing / crashing? -->
<!-- e.g. "MockRecordRef does not have ALName" or "NavSession is null when ALIsInWriteTransaction() is called" -->

## Expected behavior

<!-- What should happen in standalone mode? -->
<!-- e.g. "Return false (no write transaction in runner)", "Return empty string stub", "No-op" -->

## Likely fix

<!-- Where is the fix? Pick one: -->
- [ ] Add stub/method to `Runtime/MockXxx.cs`
- [ ] Add rewriter rule in `RoslynRewriter.cs`
- [ ] New mock class needed
- [ ] New built-in AL stub in `stubs/`
- [ ] Other: <!-- describe -->

## Acceptance criteria

- [ ] Test case `tests/NN-name/` added with positive + negative cases
- [ ] RED confirmed before fix, GREEN after
- [ ] Full regression passes
- [ ] CHANGELOG updated under `[Unreleased]`
