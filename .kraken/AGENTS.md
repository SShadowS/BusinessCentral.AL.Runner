# al-runner in kraken: how this tentacle works

This repo is the owner's fork (`origin` = SShadowS/BusinessCentral.AL.Runner). Upstream is `upstream` =
aldevtools/BusinessCentral.AL.Runner; its maintainer is StefanMaron. This tentacle exists to fix the
al-runner costs that slow the lethal project down most, and to send each fix upstream as a PR.

Follow al-runner's own `CLAUDE.md` and `docs/agents/workflow.md` first. On top of them:

1. **Work from upstream.** Cut every fix branch from `upstream/main` (`git fetch upstream` first), never
   from the `kraken` branch (it carries `.kraken/`, which must never reach a PR) and never from the
   fork's stale `main`. A PR holds only the fix commits.
2. **Local gates only** (al-runner's "PR CI is the gate"): `cargo fmt --all --check`,
   `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>`. Never `check.sh`,
   `cargo test --workspace` or a full `--all-versions` run here.
3. **Honor the maintainer's conditions** written in each issue's comments. Changing a seam needs an ADR
   the maintainer accepts; propose it in the PR or the issue, do not get ahead of it.
4. **Measure on lethal before the PR.** Ask kraken (`kraken coord ask --to owner --kind other`) to have
   the lethal tentacle measure the fix branch on Continia DO, with before/after numbers. kraken relays it.
5. **The PR (standing yes from the owner):** once the local gates pass and lethal's measurement shows the
   gain, push the fix branch to `origin` and open a DRAFT PR to aldevtools/BusinessCentral.AL.Runner
   (no auto-merge), with the measurement in its body. Then tell the owner through
   `kraken coord ask --to owner --kind other`. The maintainer lands it through the merge trains. The first
   PR will ask for a contributor-agreement signature: tell the owner, who signs it.
6. **Never** start a workflow run, push to upstream, or comment as the owner beyond the PR and its issue.

**How answers to your kraken requests are marked** (owner, 2026-11-23). kraken, the owner's coordinator, answers some requests itself on the owner's behalf:
- `[kraken, from the mantra; owner may overrule] ...` is kraken's design call from `/coord/owner/design-mantra.md`. Act on it as settled. If the owner later overrules it, follow the owner.
- `kraken: ...` or `Noted` is kraken's own work, such as a restart, an install or a relay. It is not an owner decision.
- `Owner: ...` is the owner's own words.
Secrets, money, pushes not covered by a standing yes, and new capability always go to the owner.
