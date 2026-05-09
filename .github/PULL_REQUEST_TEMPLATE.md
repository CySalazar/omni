<!--
=============================================================================
OMNI OS — Pull Request template
=============================================================================
Help reviewers help you. Fill out the sections below; delete the comments.
=============================================================================
-->

## Summary

<!--
One paragraph: what does this PR do, and why?
-->

## Type of change

<!-- Tick exactly one. Conventional Commits scope, see CONTRIBUTING.md § 4. -->

- [ ] `feat` — New feature / new public API
- [ ] `fix` — Bug fix
- [ ] `docs` — Documentation only
- [ ] `chore` — Tooling / CI / build
- [ ] `refactor` — No behavioral change
- [ ] `perf` — Performance improvement
- [ ] `test` — Tests only
- [ ] `oip` — Adds or modifies an OIP

## Conventional Commits checklist

- [ ] PR title follows `<type>(<scope>)<!>: <description>`
- [ ] Commit messages on the branch follow Conventional Commits
- [ ] If this is a **breaking change**, the title uses `!:` and a `BREAKING CHANGE:` footer is present in at least one commit

## Issue / OIP linkage

<!-- Use "Closes #N" or "Refs OIP-NNNN" as appropriate. -->

Closes #
Refs OIP-

## DCO sign-off

- [ ] All commits in this PR are signed off (`git commit -s`)

## Documentation update

<!--
Per project policy, code and docs stay in sync. If this PR changes
behavior, public APIs, wire format, or process, update the relevant doc
in the same PR.
-->

- [ ] README updated (or N/A)
- [ ] `/docs/*.md` updated (or N/A)
- [ ] Crate-level rustdoc updated (or N/A)

## Test coverage

<!--
"No tests" is rarely the right answer for new code. See CONTRIBUTING.md § 8.
-->

- [ ] Unit tests added / updated
- [ ] Property tests added / updated (where invariants apply)
- [ ] Compile-fail tests added / updated (where type-level guarantees apply)
- [ ] Bug-fix regression test added (for `fix` PRs)
- [ ] Justification provided below if any of the above is N/A

<details>
<summary>Test justification (if applicable)</summary>

<!-- Why is missing coverage acceptable? -->

</details>

## Security & privacy review

<!--
Tick the box only after self-review. Be honest — security flags caught at
PR time are cheap; production incidents are not.
-->

- [ ] No new `unsafe` code (or rationale provided below)
- [ ] No new `unwrap` / `expect` / `panic` outside `#[cfg(test)]`
- [ ] No new `disallowed-methods` or `disallowed-types` introduced
- [ ] No PII / secrets / token material can leak through new log statements
- [ ] No new dependency that fails `cargo deny check`

## Reviewer checklist (for the reviewer, not the author)

- [ ] CI is green
- [ ] Diff stays focused on a single concern
- [ ] Doc and code are consistent
- [ ] No `TBD` placeholders left in shipped paths
- [ ] If this is `area:crypto` / `area:capability` / `area:tee`, a second reviewer with security context has approved
