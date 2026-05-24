#!/usr/bin/env bash
# =============================================================================
# OMNI OS — Local repository bootstrap
# =============================================================================
# Idempotent script: initializes (or re-initializes) the local git repository,
# stages all P0 deliverables, and creates the initial commit using
# Conventional Commits.
#
# Why this script exists:
#   The Cowork sandbox where AI assistance runs cannot complete `git init`
#   because the macOS file provider blocks unlink operations on .git/*.
#   This script must be run from a regular terminal session where the
#   user has full filesystem permissions on the repository directory.
#
# Usage (from the repo root):
#   ./scripts/bootstrap-local.sh
#
# Optional environment variables:
#   GIT_USER_NAME    — overrides `git config user.name` (default: cySalazar)
#   GIT_USER_EMAIL   — overrides `git config user.email` (default: cySalazar@cySalazar.com)
#   SIGNING_KEY      — SSH public key path for signed commits (default: ~/.ssh/id_ed25519.pub)
#
# References:
#   - todo.md P0.5 (Cargo.lock + first commit)
#   - todo.md P0.7 (signed commits)
# =============================================================================

set -euo pipefail

# -----------------------------------------------------------------------------
# Pre-flight
# -----------------------------------------------------------------------------
if [[ ! -f "Cargo.toml" ]] || [[ ! -d "crates" ]]; then
  echo "error: run this script from the OMNI OS repository root." >&2
  echo "       cd to the directory containing Cargo.toml + crates/ first." >&2
  exit 64
fi

require_tool() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: required tool '$1' not found in PATH" >&2
    exit 69
  }
}

require_tool git

# -----------------------------------------------------------------------------
# Configuration (overridable via env)
# -----------------------------------------------------------------------------
GIT_USER_NAME="${GIT_USER_NAME:-cySalazar}"
GIT_USER_EMAIL="${GIT_USER_EMAIL:-cySalazar@cySalazar.com}"
SIGNING_KEY="${SIGNING_KEY:-${HOME}/.ssh/id_ed25519.pub}"

echo "==> Bootstrapping OMNI OS repository"
echo "    user.name : $GIT_USER_NAME"
echo "    user.email: $GIT_USER_EMAIL"
echo "    signing key: $SIGNING_KEY (used if commit.gpgsign=true)"
echo

# -----------------------------------------------------------------------------
# Clean up any partial init left behind by the sandbox
# -----------------------------------------------------------------------------
if [[ -d ".git" ]]; then
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    if [[ -n "$(git log --oneline 2>/dev/null || true)" ]]; then
      echo "==> Existing git repository detected with commits. Aborting to avoid"
      echo "    overwriting history. If this is intentional, remove .git manually."
      exit 1
    fi
  fi
  echo "==> Removing partial .git/ from previous attempts"
  rm -rf .git
fi

# -----------------------------------------------------------------------------
# git init
# -----------------------------------------------------------------------------
echo "==> git init -b main"
git init -b main

git config user.name  "$GIT_USER_NAME"
git config user.email "$GIT_USER_EMAIL"
git config core.autocrlf false
git config core.eol lf
git config init.defaultBranch main
git config pull.rebase true
git config push.default current

# -----------------------------------------------------------------------------
# Signing setup (SSH signing — recommended over GPG for ergonomics)
# -----------------------------------------------------------------------------
if [[ -f "$SIGNING_KEY" ]]; then
  echo "==> Configuring SSH signing with $SIGNING_KEY"
  git config gpg.format ssh
  git config user.signingkey "$SIGNING_KEY"
  git config commit.gpgsign true
  git config tag.gpgsign true

  # Initialize an allowed_signers file so `git log --show-signature` works.
  ALLOWED_SIGNERS="$HOME/.config/git/allowed_signers"
  mkdir -p "$(dirname "$ALLOWED_SIGNERS")"
  if ! grep -qF "$GIT_USER_EMAIL" "$ALLOWED_SIGNERS" 2>/dev/null; then
    echo "$GIT_USER_EMAIL $(cat "$SIGNING_KEY")" >> "$ALLOWED_SIGNERS"
    echo "    appended your key to $ALLOWED_SIGNERS"
  fi
  git config gpg.ssh.allowedSignersFile "$ALLOWED_SIGNERS"
else
  echo "==> SIGNING_KEY not found at $SIGNING_KEY"
  echo "    commits will NOT be signed. Configure manually later:"
  echo "      git config gpg.format ssh"
  echo "      git config user.signingkey <pubkey-path>"
  echo "      git config commit.gpgsign true"
  echo
  git config commit.gpgsign false
fi

# -----------------------------------------------------------------------------
# Stage and commit
# -----------------------------------------------------------------------------
echo "==> Staging all P0 deliverables"
git add -A

STAGED_COUNT=$(git diff --cached --name-only | wc -l | tr -d ' ')
echo "    $STAGED_COUNT files staged"
echo

# Compose the initial commit message — Conventional Commits + DCO sign-off.
COMMIT_MSG=$(cat <<EOF
chore(repo): initial P0 — repo hygiene and supply-chain hardening

This commit lands the Phase 0 / P0 deliverables that bring the OMNI OS
repository from a design-stage skeleton to a state ready to receive
external contributions and pass an OSS supply-chain audit.

Includes:
  - LICENSE (Apache-2.0, verbatim from FSF)
  - COMMERCIAL-LICENSE.md (placeholder, pending Stichting OMNI)
  - SECURITY.md (responsible-disclosure policy)
  - CONTRIBUTING.md (DCO, Conventional Commits, branch naming, PR flow)
  - CODE_OF_CONDUCT.md (Contributor Covenant v2.1 + escalation chain)
  - rustfmt.toml, clippy.toml, deny.toml (tool configuration)
  - .github/workflows/ (ci, audit, sbom, reproducible-build, dco, codeql, labeler)
  - .github/ISSUE_TEMPLATE/ + PULL_REQUEST_TEMPLATE.md + labeler.yml
  - .github/dependabot.yml
  - .gitattributes (LF normalization)
  - scripts/bootstrap-github.sh + scripts/bootstrap-local.sh
  - Pre-existing: README.md, /docs/01..10, /crates/* (12 skeletons), Cargo.toml,
    Cargo.lock, rust-toolchain.toml, .gitignore, todo.md

Refs todo.md P0.1 .. P0.9
Signed-off-by: $GIT_USER_NAME <$GIT_USER_EMAIL>
EOF
)

echo "==> Creating initial commit"
git commit -m "$COMMIT_MSG"

echo
echo "==> Initial commit created."
git log --oneline -1
echo
echo "==> Verifying signature (if signing was enabled)"
git log --show-signature -1 || true

# -----------------------------------------------------------------------------
# Next steps
# -----------------------------------------------------------------------------
cat <<EOF

===============================================================================
Local bootstrap complete.

Next steps:
  1. Create the GitHub repo (e.g. via 'gh repo create CySalazar/omni --public --source=. --remote=origin --push').

  2. Apply branch protection and labels from a terminal where 'gh' is
     authenticated:
       ./scripts/bootstrap-github.sh CySalazar/omni

  3. Verify protection: try pushing an unsigned commit to a test branch —
     it should be rejected.

  4. Update todo.md status icons for P0.1..P0.9 to [x] and commit.
===============================================================================
EOF
