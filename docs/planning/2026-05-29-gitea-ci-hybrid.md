# Hybrid CI — GitHub (public repo) + Gitea (CI) — Setup Guide

> Status: **proposed / in setup** (2026-05-29). Decision: founder chose the
> hybrid model — keep the public repo on GitHub for visibility/community, run CI
> on the self-hosted Gitea instance via `act_runner`. Rationale: decouple CI
> from the GitHub Actions billing block (jobs not running since ~2026-05-25)
> while preserving GitHub's public/discoverability/contributor funnel.

## 0. TL;DR

1. You **push to Gitea** (`origin`); Gitea **push-mirrors to GitHub** automatically.
2. Gitea Actions runs `.gitea/workflows/*` (already committed) on every push/PR.
3. GitHub remains the public read-only face; GitHub Actions can be re-enabled
   later (free on public repos) once the billing/payment method is fixed.

> **Cheapest alternative, check first:** GitHub Actions on **public** repos is
> free. The current block — *"recent account payments have failed"* — is an
> **account-level** payment issue, not a minutes cost. Fixing the GitHub payment
> method likely restores CI at zero cost and zero migration. Do this regardless;
> the hybrid below makes you independent of it.

## 1. Why Gitea Actions works here

Gitea Actions (Gitea ≥ 1.19) is **GitHub-Actions-syntax-compatible**: same YAML,
executed by `act_runner` (built on `nektos/act`), and `uses:` references resolve
from `https://github.com` by default — so `actions/checkout`,
`dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `EmbarkStudios/cargo-deny-action`
all run unchanged.

**Precedence:** if `.gitea/workflows/` exists, Gitea uses it and ignores
`.github/workflows/`. We added `.gitea/workflows/` so GitHub keeps using
`.github/` and Gitea uses `.gitea/` — no double-runs, clean separation.

## 2. What does NOT port (and the replacement)

| GitHub-only feature | Status on Gitea | Replacement |
|---|---|---|
| **CodeQL** (`codeql.yml`) | ❌ unsupported | `cargo audit` + (optional) `semgrep` job — see §7 |
| **Dependabot** | ❌ | **Renovate** (self-hostable, native Gitea support) — see §7 |
| **Secret scanning + push protection** | ⚠️ limited | `gitleaks` CI job — see §7 |
| `actions/attest-build-provenance` (Sigstore/OIDC) | ❌ no equivalent | drop on Gitea; keep on GitHub if/when re-enabled |
| `actions/upload-artifact@v4` | ⚠️ flaky on Gitea | use `@v3`; needs Gitea artifacts backend enabled |
| `actions/labeler` | ⚠️ partial | works for Gitea PRs; low priority |

## 3. Enable Actions on the Gitea instance

In `app.ini` on the Gitea host:

```ini
[actions]
ENABLED = true
DEFAULT_ACTIONS_URL = github   ; resolve `uses:` from github.com
```

Restart Gitea. Then in the repo: **Settings → Actions → Enable**.

## 4. Register an `act_runner`

On a host with Docker (and `/dev/kvm` if you later add the QEMU boot smoke):

```bash
# 1. Get a registration token: Gitea repo (or org/instance)
#    → Settings → Actions → Runners → "Create new runner" → copy token.
# 2. Run the runner (Docker):
docker run -d --restart=always --name omni-act-runner \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v /opt/act-runner:/data \
  -e GITEA_INSTANCE_URL="<GITEA_URL>" \
  -e GITEA_RUNNER_REGISTRATION_TOKEN="<RUNNER_TOKEN>" \
  -e GITEA_RUNNER_LABELS="ubuntu-latest:docker://catthehacker/ubuntu:act-22.04" \
  gitea/act_runner:latest
```

- The label **`ubuntu-latest`** is what the `.gitea/workflows/*` jobs request
  (`runs-on: ubuntu-latest`). The `catthehacker/ubuntu:act-*` image bundles the
  toolchain glue most actions expect (curl, git, node for JS actions).
- For the QEMU boot smoke (future), register a second runner with a privileged
  container and `/dev/kvm` mapped, labelled e.g. `kvm`.

## 5. Push-mirror Gitea → GitHub (keep GitHub public)

Make Gitea the remote you push to, and have it mirror to GitHub:

- Gitea repo → **Settings → Repository → Mirror Settings → Push Mirror**:
  - Remote URL: `https://github.com/CySalazar/omni.git`
  - Auth: a GitHub **fine-grained PAT** with `contents:write` on the repo.
  - Sync interval: e.g. `8h`, plus "Sync when commits are pushed".

Then point your local `origin` at Gitea:

```bash
git remote set-url origin <GITEA_URL>/<OWNER>/omni.git
# (optional) keep a direct GitHub remote too:
git remote add github https://github.com/CySalazar/omni.git
```

**Alternative (dual-push, no mirror):** keep `origin` on GitHub and add Gitea as
a second push URL so one `git push` hits both — Gitea then triggers Actions:

```bash
git remote set-url --add --push origin https://github.com/CySalazar/omni.git
git remote set-url --add --push origin <GITEA_URL>/<OWNER>/omni.git
```

> Note: a Gitea **pull**-mirror of GitHub does NOT reliably trigger Actions on
> sync — that's why we push *to* Gitea (push-mirror or dual-push) instead.

## 6. Branch protection / required checks on Gitea

Gitea repo → **Settings → Branches → `main` → Protect**:

- Enable **"Require status checks to pass"** and select the BLOCKING jobs:
  `cargo test (workspace, excl. omni-kernel)`, `bare-metal build
  (x86_64-unknown-none)`, `kernel-runner build (x86_64-unknown-none)`,
  `blanket #![allow] guard (ADR-0003)`, `cargo deny`, and `oip-lint` /
  `DCO sign-off` for the relevant paths.
- Do **not** require the report-only jobs (`cargo fmt`, `cargo clippy`,
  `cargo doc`, `cargo test omni-kernel`) until their debt is cleared (§8).
- Keep "Require signed commits" if you want parity with the GitHub setup.

## 7. Replacements for the GitHub-only security tooling

- **CodeQL → cargo-audit / semgrep.** Add a `security` job:
  `cargo install cargo-audit && cargo audit` (advisories), optionally a
  `semgrep ci` step with a Rust ruleset. (Not yet committed — add when ready.)
- **Dependabot → Renovate.** Self-host the Renovate bot (or run it from the
  Gitea Actions runner on a cron). Renovate supports Gitea natively
  (`platform: gitea`). Add a `renovate.json` at repo root.
- **Secret scanning → gitleaks.** Add a `gitleaks` job
  (`uses: gitleaks/gitleaks-action`, or run the binary) on push/PR.

## 8. From REPORT-ONLY to STRICT (debt to clear first)

The committed `.gitea/workflows/ci.yml` runs `fmt`, `clippy`, `doc`, and the
kernel test as **`continue-on-error: true`** because of debt that accumulated
while CI was down:

1. **fmt:** ~692 nightly-rustfmt diffs across crates (omni-agent et al.). Fix
   with one repo-wide `cargo +nightly fmt --all` commit, then drop
   `continue-on-error` from the `fmt` job + flip the local pre-push hook strict
   block.
2. **clippy:** ~82 `-D warnings` violations (Sprint-10 files: tensor_loader,
   speculative, gguf, model_loader, bpe, tests/benches) **+ 6 deny-level
   correctness errors in omni-hal** ("operation will always return zero").
   Triage, then drop `continue-on-error` from `clippy`.
3. **doc:** verify `cargo doc -D warnings` is clean, then drop its flag.
4. **kernel test SIGSEGV:** the `omni-kernel --lib` crash on x86_64 Linux dev
   hosts (see memory `preexisting_test_sigsegv`). If it does not reproduce on
   the Gitea runner, fold `omni-kernel` back into the main `test` job and remove
   the separate `test-kernel` report-only job.

Each flip makes the Gitea gate a strict equivalent of the original GitHub CI.

## 9. Local safety net (already installed)

`doe-framework/templates/git-hooks/pre-push` (installed to `.git/hooks/pre-push`)
runs the same gates locally before every push: tests + deny(bans/licenses/
sources) BLOCKING, fmt/clippy/advisories REPORT-ONLY. Bypass with
`git push --no-verify`; fast mode with `OMNI_PREPUSH_FAST=1`. Reinstall after a
fresh clone via the DOE installer's `hooks` command.

## 10. Founder-side TODO (info needed to finalize)

- [ ] Decide: fix GitHub billing (free public Actions) AND/OR proceed with Gitea.
- [ ] Provide the Gitea instance URL + create a runner registration token.
- [ ] Run the `act_runner` (§4) and configure the push-mirror (§5).
- [ ] Set branch protection required checks on Gitea (§6).
- [ ] (Later) add security replacements (§7) and clear the debt to go strict (§8).
