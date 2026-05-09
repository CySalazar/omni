#!/usr/bin/env bash
# =============================================================================
# OMNI OS — GitHub repository bootstrap script
# =============================================================================
# Idempotent script that configures a freshly-pushed GitHub repository to
# match the OMNI OS P0 policy:
#
#   - main as default branch, force-push disabled, deletion disabled
#   - Required PR review (2 approvers, drops to 1 until co-maintainer joins)
#   - Required signed commits
#   - Required status checks (CI workflows)
#   - Linear history enforced
#   - Stale reviews dismissed on push
#   - Tag protection (signed only, mergeable from main)
#   - Default labels created (area:*, priority:*, kind:*)
#
# Prerequisites:
#   - gh CLI (https://cli.github.com/) authenticated to an account with
#     admin rights on the target repo
#   - The local repo is already pushed to origin (default `origin/main`)
#
# Usage:
#   ./scripts/bootstrap-github.sh <owner>/<repo>
#
# Example:
#   ./scripts/bootstrap-github.sh CySalazar/omni
#
# References:
#   - todo.md P0.7 (branch protection + signed commits)
#   - todo.md P0.8 (label taxonomy)
# =============================================================================

set -euo pipefail

# -----------------------------------------------------------------------------
# Argument validation
# -----------------------------------------------------------------------------
if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <owner>/<repo>" >&2
  echo "Example: $0 CySalazar/omni" >&2
  exit 64  # EX_USAGE
fi

REPO="$1"

# -----------------------------------------------------------------------------
# Pre-flight: tool availability and authentication
# -----------------------------------------------------------------------------
require_tool() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: required tool '$1' not found in PATH" >&2
    exit 69  # EX_UNAVAILABLE
  }
}

require_tool gh
require_tool jq

# Validates gh is authenticated; exits 1 if not.
gh auth status >/dev/null

# -----------------------------------------------------------------------------
# Settings: default branch, force-push, delete, merge strategy
# -----------------------------------------------------------------------------
echo "==> Configuring repository settings for $REPO"
gh repo edit "$REPO" \
  --default-branch main \
  --enable-issues \
  --enable-discussions \
  --enable-projects \
  --enable-wiki=false \
  --enable-merge-commit=false \
  --enable-squash-merge=true \
  --enable-rebase-merge=false \
  --delete-branch-on-merge=true

# -----------------------------------------------------------------------------
# Branch protection on `main`
# -----------------------------------------------------------------------------
# We use the GitHub REST API directly because gh CLI lacks first-class flags
# for required signatures and required status checks at the time of writing.
echo "==> Applying branch protection on main"

# NOTE: required_approving_review_count drops to 1 until a co-maintainer joins.
# Bump back to 2 in this script once Phase 1 hiring lands (todo.md P4.4).
APPROVING_REVIEWS=1

cat > /tmp/branch-protection.json <<JSON
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "ci / cargo fmt",
      "ci / cargo clippy",
      "ci / cargo test (ubuntu-24.04)",
      "ci / cargo doc",
      "audit / cargo audit",
      "audit / cargo deny",
      "dco / DCO sign-off",
      "codeql / CodeQL — rust"
    ]
  },
  "enforce_admins": true,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": false,
    "required_approving_review_count": ${APPROVING_REVIEWS},
    "require_last_push_approval": true
  },
  "restrictions": null,
  "required_linear_history": true,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "block_creations": false,
  "required_conversation_resolution": true,
  "lock_branch": false,
  "allow_fork_syncing": true,
  "required_signatures": true
}
JSON

gh api -X PUT \
  -H "Accept: application/vnd.github+json" \
  "repos/${REPO}/branches/main/protection" \
  --input /tmp/branch-protection.json >/dev/null

rm -f /tmp/branch-protection.json
echo "    main is protected (signed commits + ${APPROVING_REVIEWS} reviewer + linear history)."

# -----------------------------------------------------------------------------
# Tag protection rule
# -----------------------------------------------------------------------------
# Note: Tag protection is being deprecated in favor of Rulesets; this block
# uses the legacy endpoint that still works at time of writing. Migrate to
# rulesets once stable.
echo "==> Applying tag protection (v*.*.* pattern)"
gh api -X POST \
  -H "Accept: application/vnd.github+json" \
  "repos/${REPO}/tags/protection" \
  -f pattern='v*.*.*' >/dev/null 2>&1 || \
  echo "    (tag protection rule may already exist or endpoint deprecated; manual setup may be required)"

# -----------------------------------------------------------------------------
# Default labels — area / priority / kind / special
# -----------------------------------------------------------------------------
echo "==> Creating default label taxonomy"

create_label() {
  local name="$1"
  local color="$2"
  local description="$3"
  gh api -X POST \
    -H "Accept: application/vnd.github+json" \
    "repos/${REPO}/labels" \
    -f "name=${name}" \
    -f "color=${color}" \
    -f "description=${description}" >/dev/null 2>&1 || \
  gh api -X PATCH \
    -H "Accept: application/vnd.github+json" \
    "repos/${REPO}/labels/${name}" \
    -f "new_name=${name}" \
    -f "color=${color}" \
    -f "description=${description}" >/dev/null
  echo "    label: ${name}"
}

# area:* — by code path
create_label "area:kernel"       "1d76db" "omni-kernel crate"
create_label "area:crypto"       "5319e7" "omni-crypto crate"
create_label "area:capability"   "5319e7" "omni-capability crate"
create_label "area:tee"          "5319e7" "omni-tee crate / TEE backends"
create_label "area:hal"          "1d76db" "omni-hal crate"
create_label "area:runtime"      "1d76db" "omni-runtime crate"
create_label "area:mesh"         "0e8a16" "omni-mesh crate"
create_label "area:tokenization" "5319e7" "omni-tokenization crate"
create_label "area:sdk"          "fbca04" "omni-sdk crate"
create_label "area:agent"        "fbca04" "omni-agent crate"
create_label "area:shell"        "fbca04" "omni-shell crate"
create_label "area:types"        "1d76db" "omni-types crate"
create_label "area:docs"         "0075ca" "Documentation under /docs or top-level"
create_label "area:ci"           "ededed" "CI / tooling / .github"
create_label "area:oip"          "8a2be2" "OIP draft or amendment"

# priority:*
create_label "priority:P0" "b60205" "Critical — blocks release / merges"
create_label "priority:P1" "d93f0b" "High — current iteration"
create_label "priority:P2" "fbca04" "Medium — next iteration"
create_label "priority:P3" "0e8a16" "Low — backlog"

# kind:*
create_label "kind:bug"      "d73a4a" "Defect"
create_label "kind:feature"  "a2eeef" "Feature request"
create_label "kind:refactor" "fef2c0" "Refactor with no behavioral change"
create_label "kind:docs"     "0075ca" "Documentation only"
create_label "kind:security" "ee0701" "Security-relevant — handle with care"
create_label "kind:chore"    "ededed" "Tooling / CI / dependencies"

# Special
create_label "oip-required"     "8a2be2" "Substantive change requiring an OIP"
create_label "breaking-change"  "b60205" "Breaks public API or wire format"
create_label "good-first-issue" "7057ff" "Good for newcomers"
create_label "help-wanted"      "008672" "Maintainers welcome external contribution"
create_label "needs-triage"     "d4c5f9" "Awaits triage by maintainer"
create_label "dependencies"     "0366d6" "Dependency bump (auto-applied by Dependabot)"
create_label "do-not-use"       "000000" "Reserved label, do not apply manually"

# -----------------------------------------------------------------------------
# Vulnerability alerts + automated security fixes
# -----------------------------------------------------------------------------
echo "==> Enabling vulnerability alerts and automated security fixes"
gh api -X PUT \
  -H "Accept: application/vnd.github+json" \
  "repos/${REPO}/vulnerability-alerts" >/dev/null
gh api -X PUT \
  -H "Accept: application/vnd.github+json" \
  "repos/${REPO}/automated-security-fixes" >/dev/null

# -----------------------------------------------------------------------------
# Secret scanning + push protection
# -----------------------------------------------------------------------------
# Available on public repos by default; explicit PATCH for clarity.
echo "==> Enabling secret scanning + push protection"
gh api -X PATCH \
  -H "Accept: application/vnd.github+json" \
  "repos/${REPO}" \
  --input - <<JSON >/dev/null
{
  "security_and_analysis": {
    "secret_scanning": { "status": "enabled" },
    "secret_scanning_push_protection": { "status": "enabled" }
  }
}
JSON

# -----------------------------------------------------------------------------
# Done
# -----------------------------------------------------------------------------
cat <<EOF

===============================================================================
Bootstrap complete for $REPO

Next steps for the human:
  1. Configure your local git to sign commits (recommended: SSH signing):

       git config --global user.signingkey ~/.ssh/id_ed25519.pub
       git config --global gpg.format ssh
       git config --global commit.gpgsign true
       git config --global tag.gpgsign true

     Then add your SSH key as a "Signing Key" in GitHub settings:
       https://github.com/settings/ssh/new?type=signing

  2. Verify the protection took effect by attempting an unsigned push to a
     test branch — it should be rejected.

  3. Consider enabling Discussions categories — Q-and-A, Ideas, OIP-staging.
===============================================================================
EOF
