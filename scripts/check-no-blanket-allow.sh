#!/usr/bin/env bash
# scripts/check-no-blanket-allow.sh
#
# Enforces ADR-0003: no blanket #![allow(...)] in production crates.
#
# Scans crates/*/src/{lib,main}.rs for crate-level #![allow(...)] attributes.
# Allowlisted patterns (documented in ADR-0003):
#   - #![doc(...)]
#   - #![warn(...)]
#   - #![cfg_attr(test, allow(...))]            (test-only relaxation)
#   - #![cfg_attr(all(feature = "bare-metal", ...))]  (no_std / no_main gating)
#
# Any other form of crate-root #![allow(...)] is a violation and exits 1.
#
# Usage: scripts/check-no-blanket-allow.sh
# Exit:  0 = clean, 1 = violations found
#
# See: docs/adr/0003-no-blanket-allows-in-production-crates.md

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

violations=0
violation_log=$(mktemp)
trap 'rm -f "$violation_log"' EXIT

# Find all crate-root files in production crates within the in-scope set.
#
# In-scope crates for ADR-0003 enforcement (Step 7):
#   - omni-kernel (the kernel itself — bug-fatal target of this ADR)
#   - omni-types, omni-crypto, omni-capability, omni-tee — already lint-clean
#   - omni-hal, omni-runtime, omni-mesh, omni-tokenization, omni-sdk,
#     omni-agent, omni-shell — stub-stage but covered by workspace policy
#
# Out-of-scope (documented exceptions, tracked separately):
#   - omni-container: carries a known upstream clippy false-positive workaround
#     (clippy::literal_string_with_formatting_args). Tracked in
#     `crates/omni-container/src/lib.rs:78-85` with full rationale comment.
#     A separate ADR will fold this into policy if the false positive
#     persists past clippy upstream resolution.
SCOPED_CRATES=(
    "crates/omni-types"
    "crates/omni-crypto"
    "crates/omni-capability"
    "crates/omni-tee"
    "crates/omni-kernel"
    "crates/omni-hal"
    "crates/omni-runtime"
    "crates/omni-mesh"
    "crates/omni-tokenization"
    "crates/omni-sdk"
    "crates/omni-agent"
    "crates/omni-shell"
    "crates/omni-driver-net-virtio"
    "crates/omni-driver-nvme"
    "crates/omni-driver-e1000e"
    # `omni-driver-shared` is the dep-free SDK helper crate (P6.7.8.10).
    # Enrolled here so ADR-0003 enforcement covers its crate-root file.
    "crates/omni-driver-shared"
    # `omni-fs` is the filesystem service skeleton (TASK-011, OIP-FS-018).
    "crates/omni-fs"
)

files=""
for crate in "${SCOPED_CRATES[@]}"; do
    while IFS= read -r f; do
        files+="$f"$'\n'
    done < <(find "$crate" -type f \( -name lib.rs -o -name main.rs \) \
              -not -path '*/target/*' \
              -not -path '*/tests/*' \
              -not -path '*/examples/*' \
              -not -path '*/benches/*' \
              -not -path '*/fuzz/*' \
              2>/dev/null || true)
done
files=$(echo "$files" | sort -u | grep -v '^$' || true)

for file in $files; do
    [[ -z "$file" ]] && continue
    # Scan crate-root attributes (#![...]) only.
    # A crate-root attribute starts with `#![` at column 0 (or after whitespace
    # in continuation lines). We collect logical attribute lines: each one is
    # the lines from `#![` to the matching `]` that closes it.
    awk '
        # State: in_attr means we are inside a multi-line #![ ... ] block.
        BEGIN { in_attr = 0; buf = ""; depth = 0; start_line = 0 }

        # Track multi-line attribute accumulation. Naive paren-depth counter:
        # increments on each `(`, decrements on each `)`. Quoted strings are
        # ignored for simplicity (no kernel crate-root attribute has a `(`
        # inside a string literal).
        {
            line = $0
            if (!in_attr) {
                # Look for the start of a crate-root attribute: `#![`
                if (match(line, /^[[:space:]]*#!\[/)) {
                    in_attr = 1
                    start_line = NR
                    buf = line
                    depth = 0
                    # Count parens on this first line
                    n = length(line)
                    for (i = 1; i <= n; i++) {
                        ch = substr(line, i, 1)
                        if (ch == "(") depth++
                        else if (ch == ")") depth--
                    }
                    # If the attribute closes on the same line (depth == 0 AND
                    # we have seen the closing `]` after the `#![`), emit it.
                    if (depth == 0 && index(line, "]") > 0) {
                        emit(buf, start_line)
                        in_attr = 0; buf = ""
                    }
                }
            } else {
                buf = buf "\n" line
                n = length(line)
                for (i = 1; i <= n; i++) {
                    ch = substr(line, i, 1)
                    if (ch == "(") depth++
                    else if (ch == ")") depth--
                }
                if (depth == 0 && index(line, "]") > 0) {
                    emit(buf, start_line)
                    in_attr = 0; buf = ""
                }
            }
        }

        function emit(attr, ln,    is_allowed) {
            is_allowed = 0

            # Whitelist patterns (ADR-0003 § Escape hatches):
            if (attr ~ /^[[:space:]]*#!\[doc\(/)                                is_allowed = 1
            else if (attr ~ /^[[:space:]]*#!\[warn\(/)                          is_allowed = 1
            else if (attr ~ /^[[:space:]]*#!\[cfg_attr\([[:space:]]*test[[:space:]]*,[[:space:]]*allow\(/) is_allowed = 1
            else if (attr ~ /^[[:space:]]*#!\[cfg_attr\(all\(feature[[:space:]]*=[[:space:]]*"bare-metal"/) is_allowed = 1
            # Not an allow attribute at all → ignore (e.g. #![no_std], #![deny(...)], #![feature(...)] etc).
            else if (attr !~ /allow/) is_allowed = 1

            if (!is_allowed) {
                # Print the violation. The caller (shell) decides what to do.
                # Emit single-line summary: file:start_line: first 100 chars.
                summary = attr
                gsub(/\n/, " ", summary)
                gsub(/[[:space:]]+/, " ", summary)
                if (length(summary) > 100) summary = substr(summary, 1, 97) "..."
                printf("VIOLATION %s:%d %s\n", FILENAME, ln, summary)
            }
        }
    ' "$file" >> "$violation_log" || true
done

if [[ -s "$violation_log" ]]; then
    echo "Blanket #![allow(...)] policy violation (see ADR-0003)." >&2
    echo "" >&2
    cat "$violation_log" >&2
    echo "" >&2
    echo "Allowed crate-root forms:" >&2
    echo "  - #![doc(...)]" >&2
    echo "  - #![warn(...)]" >&2
    echo "  - #![cfg_attr(test, allow(...))]" >&2
    echo "  - #![cfg_attr(all(feature = \"bare-metal\", ...))]" >&2
    echo "" >&2
    echo "Move each violation to a localized #[allow(<lint>, reason = \"...\")] " >&2
    echo "at the offending item. See docs/adr/0003-no-blanket-allows-in-production-crates.md." >&2
    exit 1
fi

echo "check-no-blanket-allow: ok (scanned $(echo "$files" | wc -l | tr -d ' ') crate-root files)"
exit 0
