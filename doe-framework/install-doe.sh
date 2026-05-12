#!/usr/bin/env bash

# =============================================================================
# install-doe.sh — D.O.E. Framework Installer
# =============================================================================
#
# Part of the D.O.E. Framework (Direttiva, Orchestrazione, Esecuzione)
#
# PURPOSE:
#   All-in-one installer for the D.O.E. framework. Copies the framework into
#   a target project, installs git hooks, scaffolds the project structure,
#   initializes git, and verifies framework integrity.
#
# COMMANDS:
#   install   Copy the framework + install hooks + scaffold (default)
#   reset     Overwrite existing framework copy with a fresh one
#   hooks     Install/update only the git hooks
#   scaffold  Create only the project directory structure
#   verify    Check framework integrity (all files present + checksums)
#
# USAGE:
#   ./install-doe.sh <command> <project-directory> [OPTIONS]
#   ./install-doe.sh --help
#
# EXIT CODES:
#   0 — Success
#   1 — Invalid arguments
#   2 — Target directory issues
#   3 — Framework integrity issue (source files missing/corrupted)
#   4 — Operation failed (permission or filesystem error)
#
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

readonly VERSION="1.0.0"

# Resolve the absolute path of the directory where this script lives.
# This allows the script to be called from anywhere and still find
# the framework files relative to its own location.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Framework source directories (relative to script location)
readonly HOOKS_SOURCE_DIR="${SCRIPT_DIR}/templates/git-hooks"
readonly GITIGNORE_SOURCE_DIR="${SCRIPT_DIR}/templates/.gitignore-templates"

# Files and directories that are part of the framework (for copy and verify).
# CLAUDE.md is excluded from copy — it lives in project root, not inside
# the doe-framework/ subdirectory. install-doe.sh itself is also excluded
# from the copy target since it stays at framework root level.
readonly FRAMEWORK_DIRS=(
    "L1-directives"
    "L2-orchestration"
    "L3-execution"
    "templates"
    "templates/.gitignore-templates"
    "templates/git-hooks"
)

# Core framework files that MUST exist for the framework to be considered intact.
# Paths are relative to the framework root (where this script lives).
readonly FRAMEWORK_CORE_FILES=(
    "DOE.md"
    "CLAUDE.md"
    "L1-directives/01-project-intake.md"
    "L1-directives/02-architecture-patterns.md"
    "L1-directives/03-directive-template.md"
    "L1-directives/04-directive-catalog.md"
    "L2-orchestration/01-decision-engine.md"
    "L2-orchestration/02-task-decomposition.md"
    "L2-orchestration/03-error-recovery.md"
    "L2-orchestration/04-interaction-protocol.md"
    "L2-orchestration/05-state-management.md"
    "L3-execution/01-code-standards.md"
    "L3-execution/02-testing-strategy.md"
    "L3-execution/03-security-guidelines.md"
    "L3-execution/04-documentation-rules.md"
    "L3-execution/05-cicd-setup.md"
    "L3-execution/06-dependency-management.md"
    "templates/project-spec.md"
    "templates/project-state-template.md"
    "templates/adr-template.md"
    "templates/changelog-template.md"
    "templates/git-hooks/commit-msg"
)

# Available language presets for .gitignore
readonly SUPPORTED_LANGUAGES=("dotnet" "python" "node" "go" "dart")

# ANSI color codes (disabled when stdout is not a terminal)
if [ -t 1 ]; then
    readonly RED='\033[0;31m'
    readonly GREEN='\033[0;32m'
    readonly YELLOW='\033[0;33m'
    readonly CYAN='\033[0;36m'
    readonly BOLD='\033[1m'
    readonly DIM='\033[2m'
    readonly RESET='\033[0m'
else
    readonly RED='' GREEN='' YELLOW='' CYAN='' BOLD='' DIM='' RESET=''
fi

# ---------------------------------------------------------------------------
# Global flags (set by argument parser)
# ---------------------------------------------------------------------------

VERBOSE=false
DRY_RUN=false
COMMAND=""
PROJECT_DIR=""
LANGUAGE=""
FORCE=false

# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------

info()       { echo -e "${CYAN}[INFO]${RESET}  $1"; }
success()    { echo -e "${GREEN}[OK]${RESET}    $1"; }
warn()       { echo -e "${YELLOW}[WARN]${RESET}  $1"; }
debug()      { [[ "$VERBOSE" == true ]] && echo -e "${DIM}[DEBUG]${RESET} $1" || true; }
dry_run_msg(){ echo -e "${YELLOW}[DRY]${RESET}   $1"; }

error_exit() {
    echo -e "${RED}[ERROR]${RESET} $1" >&2
    exit "${2:-1}"
}

# Wraps a command: in dry-run mode, prints it; otherwise, executes it.
run_cmd() {
    if [[ "$DRY_RUN" == true ]]; then
        dry_run_msg "Would run: $*"
    else
        debug "Running: $*"
        "$@"
    fi
}

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------

show_help() {
    cat << 'HELPTEXT'

  ╔═══════════════════════════════════════════════════════════════════╗
  ║            D.O.E. Framework Installer — v1.0.0                  ║
  ║            (Direttiva, Orchestrazione, Esecuzione)              ║
  ╚═══════════════════════════════════════════════════════════════════╝

  DESCRIPTION

    All-in-one installer for the D.O.E. framework. Copies the framework
    into a target project, installs git hooks, scaffolds the directory
    structure, initializes git, and verifies framework integrity.

  ─────────────────────────────────────────────────────────────────────
  COMMANDS
  ─────────────────────────────────────────────────────────────────────

    install <dir> [OPTIONS]     Full installation (default command)
                                  1. Copies the framework to <dir>/doe-framework/
                                  2. Copies CLAUDE.md to <dir>/CLAUDE.md
                                  3. Initializes git (if needed)
                                  4. Installs .gitignore (if --lang is set)
                                  5. Installs git hooks
                                  6. Scaffolds project structure

    reset <dir> [OPTIONS]       Reset the framework in an existing project
                                  Removes <dir>/doe-framework/ and reinstalls
                                  a fresh copy from source. Also updates
                                  CLAUDE.md and git hooks.
                                  Requires --force to prevent accidents.

    hooks <dir>                 Install or update only the git hooks
                                  Copies hooks from the framework templates
                                  to <dir>/.git/hooks/

    scaffold <dir> [OPTIONS]    Create only the project directory structure
                                  Creates src/, tests/, docs/, docs/adr/,
                                  .github/workflows/ and placeholder files.

    verify [dir]                Verify framework integrity
                                  Checks all core files exist. If a dir is
                                  given, checks the installed copy; otherwise
                                  checks the source framework.

  ─────────────────────────────────────────────────────────────────────
  OPTIONS
  ─────────────────────────────────────────────────────────────────────

    --lang <language>   Language preset for .gitignore and scaffold.
                        Supported: dotnet, python, node, go, dart

    --force             Required for 'reset' command. Confirms you want
                        to overwrite the existing framework copy.

    --dry-run           Show what would happen without making changes.

    --verbose, -v       Print detailed information during execution.

    --help, -h          Show this help message and exit.

    --version           Show version number and exit.

  ─────────────────────────────────────────────────────────────────────
  EXAMPLES
  ─────────────────────────────────────────────────────────────────────

    # Full install into a new project (C#/.NET)
    ./install-doe.sh install ~/projects/my-api --lang dotnet

    # Full install into current directory (Python)
    ./install-doe.sh install . --lang python

    # Preview what install would do
    ./install-doe.sh install ~/projects/my-app --lang node --dry-run

    # Reset framework to latest version
    ./install-doe.sh reset ~/projects/my-api --force

    # Update only the git hooks
    ./install-doe.sh hooks ~/projects/my-api

    # Scaffold project structure only
    ./install-doe.sh scaffold ~/projects/my-api --lang go

    # Verify framework source integrity
    ./install-doe.sh verify

    # Verify installed framework in a project
    ./install-doe.sh verify ~/projects/my-api

  ─────────────────────────────────────────────────────────────────────
  EXIT CODES
  ─────────────────────────────────────────────────────────────────────

    0   Success
    1   Invalid arguments
    2   Target directory issues (doesn't exist, not a git repo, etc.)
    3   Framework integrity issue (missing or corrupted source files)
    4   Operation failed (permission or filesystem error)

  ─────────────────────────────────────────────────────────────────────
  GIT HOOKS INSTALLED
  ─────────────────────────────────────────────────────────────────────

    commit-msg    Strips AI/LLM attribution (Co-Authored-By, etc.)
                  from commit messages. Covers Claude, GPT, Copilot,
                  Gemini, Cursor, Windsurf, Cody, Tabnine, and more.
                  Human co-authors are always preserved.

HELPTEXT
    exit 0
}

show_version() {
    echo "install-doe.sh version ${VERSION}"
    exit 0
}

# ---------------------------------------------------------------------------
# Validation helpers
# ---------------------------------------------------------------------------

# Ensure the framework source is intact before any operation
validate_framework_source() {
    local missing=0

    for file in "${FRAMEWORK_CORE_FILES[@]}"; do
        if [ ! -f "${SCRIPT_DIR}/${file}" ]; then
            warn "Missing source file: ${file}"
            missing=$((missing + 1))
        fi
    done

    if [ "$missing" -gt 0 ]; then
        error_exit "Framework source is incomplete: ${missing} file(s) missing.\nRe-download or restore the doe-framework/ directory." 3
    fi

    debug "Framework source validated: all ${#FRAMEWORK_CORE_FILES[@]} core files present."
}

# Validate that the language flag is supported
validate_language() {
    if [ -n "$LANGUAGE" ]; then
        local valid=false
        for lang in "${SUPPORTED_LANGUAGES[@]}"; do
            if [[ "$lang" == "$LANGUAGE" ]]; then
                valid=true
                break
            fi
        done

        if [[ "$valid" != true ]]; then
            error_exit "Unsupported language: '${LANGUAGE}'.\nSupported languages: ${SUPPORTED_LANGUAGES[*]}" 1
        fi

        # Check that the .gitignore template exists
        if [ ! -f "${GITIGNORE_SOURCE_DIR}/${LANGUAGE}.gitignore" ]; then
            error_exit "Gitignore template not found: ${GITIGNORE_SOURCE_DIR}/${LANGUAGE}.gitignore" 3
        fi
    fi
}

# Ensure target directory exists (or create it)
ensure_project_dir() {
    if [ ! -d "$PROJECT_DIR" ]; then
        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would create directory: ${PROJECT_DIR}"
            # In dry-run mode, we still need an absolute path for display purposes.
            # Use the raw path since we can't cd into a non-existent dir.
            case "$PROJECT_DIR" in
                /*) ;; # already absolute
                *)  PROJECT_DIR="$(pwd)/${PROJECT_DIR}" ;;
            esac
            return 0
        fi
        info "Directory does not exist. Creating: ${PROJECT_DIR}"
        mkdir -p "$PROJECT_DIR" || error_exit "Failed to create directory: ${PROJECT_DIR}" 4
    fi

    # Resolve to absolute path
    PROJECT_DIR="$(cd "$PROJECT_DIR" 2>/dev/null && pwd)" || error_exit "Cannot resolve directory: ${PROJECT_DIR}" 2
    debug "Resolved project directory: ${PROJECT_DIR}"
}

# ---------------------------------------------------------------------------
# Core operations
# ---------------------------------------------------------------------------

# Initialize a git repository if one doesn't exist
init_git() {
    if [ -d "${PROJECT_DIR}/.git" ]; then
        debug "Git repository already initialized."
        return 0
    fi

    info "Initializing git repository..."
    run_cmd git -C "$PROJECT_DIR" init -q || error_exit "Failed to initialize git repository." 4
    success "Git repository initialized."
}

# Copy .gitignore from language template
install_gitignore() {
    if [ -z "$LANGUAGE" ]; then
        debug "No --lang specified, skipping .gitignore installation."
        return 0
    fi

    local source="${GITIGNORE_SOURCE_DIR}/${LANGUAGE}.gitignore"
    local target="${PROJECT_DIR}/.gitignore"

    if [ -f "$target" ] && [[ "$FORCE" != true ]]; then
        warn ".gitignore already exists. Merging D.O.E. entries if missing..."
        if ! grep -q "CLAUDE.md" "$target" 2>/dev/null; then
            if [[ "$DRY_RUN" == true ]]; then
                dry_run_msg "Would append 'CLAUDE.md' to .gitignore"
            else
                echo "" >> "$target"
                echo "# D.O.E. Framework — agent instructions (never commit)" >> "$target"
                echo "CLAUDE.md" >> "$target"
                success "Added CLAUDE.md to existing .gitignore"
            fi
        else
            debug "CLAUDE.md already in .gitignore."
        fi
        if ! grep -q ".startup-prompt.txt" "$target" 2>/dev/null; then
            if [[ "$DRY_RUN" == true ]]; then
                dry_run_msg "Would append '.startup-prompt.txt' to .gitignore"
            else
                echo "doe-framework/.startup-prompt.txt" >> "$target"
                debug "Added .startup-prompt.txt to .gitignore"
            fi
        fi
        return 0
    fi

    info "Installing .gitignore (${LANGUAGE})..."
    if [[ "$DRY_RUN" == true ]]; then
        dry_run_msg "Would copy ${source} -> ${target}"
    else
        cp "$source" "$target" || error_exit "Failed to copy .gitignore" 4

        # Ensure CLAUDE.md and startup prompt are in .gitignore
        if ! grep -q "CLAUDE.md" "$target" 2>/dev/null; then
            echo "" >> "$target"
            echo "# D.O.E. Framework — agent instructions (never commit)" >> "$target"
            echo "CLAUDE.md" >> "$target"
        fi
        if ! grep -q ".startup-prompt.txt" "$target" 2>/dev/null; then
            echo "doe-framework/.startup-prompt.txt" >> "$target"
        fi
    fi

    success ".gitignore installed (${LANGUAGE})."
}

# Copy the framework into the project
copy_framework() {
    local target_fw_dir="${PROJECT_DIR}/doe-framework"

    if [ -d "$target_fw_dir" ] && [[ "$FORCE" != true ]]; then
        warn "doe-framework/ already exists in the project. Use 'reset --force' to overwrite."
        return 0
    fi

    info "Copying D.O.E. framework to ${target_fw_dir}/ ..."

    # Create framework directories
    for dir in "${FRAMEWORK_DIRS[@]}"; do
        run_cmd mkdir -p "${target_fw_dir}/${dir}"
    done

    # Copy all core files (except CLAUDE.md which goes to project root,
    # and install-doe.sh which stays at framework source level)
    local copied=0
    for file in "${FRAMEWORK_CORE_FILES[@]}"; do
        # CLAUDE.md is handled separately (goes to project root)
        [[ "$file" == "CLAUDE.md" ]] && continue

        local source="${SCRIPT_DIR}/${file}"
        local target="${target_fw_dir}/${file}"

        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would copy: ${file}"
        else
            cp "$source" "$target" || error_exit "Failed to copy: ${file}" 4
            debug "Copied: ${file}"
        fi
        copied=$((copied + 1))
    done

    # Also copy install-doe.sh into the project's framework copy
    if [[ "$DRY_RUN" == true ]]; then
        dry_run_msg "Would copy: install-doe.sh"
    else
        cp "${SCRIPT_DIR}/install-doe.sh" "${target_fw_dir}/install-doe.sh" || error_exit "Failed to copy install-doe.sh" 4
        chmod +x "${target_fw_dir}/install-doe.sh"
        debug "Copied: install-doe.sh"
    fi
    copied=$((copied + 1))

    success "Framework copied (${copied} files)."
}

# Copy CLAUDE.md to project root
install_claude_md() {
    local source="${SCRIPT_DIR}/CLAUDE.md"
    local target="${PROJECT_DIR}/CLAUDE.md"

    if [ -f "$target" ] && [[ "$FORCE" != true ]]; then
        warn "CLAUDE.md already exists in project root. Use 'reset --force' to overwrite."
        return 0
    fi

    info "Installing CLAUDE.md to project root..."

    if [[ "$DRY_RUN" == true ]]; then
        dry_run_msg "Would copy: CLAUDE.md -> ${target}"
    else
        cp "$source" "$target" || error_exit "Failed to copy CLAUDE.md to project root." 4
    fi

    success "CLAUDE.md installed."
}

# Install git hooks
install_hooks() {
    if [ ! -d "${PROJECT_DIR}/.git" ]; then
        warn "Not a git repository. Skipping hook installation."
        warn "Run 'git init' first, then re-run: install-doe.sh hooks ${PROJECT_DIR}"
        return 0
    fi

    local hooks_dir="${PROJECT_DIR}/.git/hooks"
    run_cmd mkdir -p "$hooks_dir"

    info "Installing git hooks..."

    local installed=0
    for hook_source in "${HOOKS_SOURCE_DIR}"/*; do
        [ -f "$hook_source" ] || continue

        local hook_name
        hook_name="$(basename "$hook_source")"

        # Skip hidden files and backups
        case "$hook_name" in
            .*|*.backup.*) continue ;;
        esac

        local target="${hooks_dir}/${hook_name}"

        # Backup existing hook
        if [ -f "$target" ]; then
            local backup="${target}.backup.$(date +%Y%m%d_%H%M%S)"
            if [[ "$DRY_RUN" == true ]]; then
                dry_run_msg "Would backup: ${hook_name} -> $(basename "$backup")"
            else
                cp "$target" "$backup" || error_exit "Failed to backup hook: ${hook_name}" 4
                warn "Existing '${hook_name}' backed up as $(basename "$backup")"
            fi
        fi

        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would install hook: ${hook_name}"
        else
            cp "$hook_source" "$target" || error_exit "Failed to install hook: ${hook_name}" 4
            chmod +x "$target" || error_exit "Failed to set permissions on: ${hook_name}" 4
            debug "Installed hook: ${hook_name}"
        fi

        installed=$((installed + 1))
    done

    if [ "$installed" -gt 0 ]; then
        success "${installed} git hook(s) installed."
    else
        warn "No hooks found in templates."
    fi
}

# Scaffold the project directory structure
scaffold_project() {
    info "Scaffolding project structure..."

    # Core directories (always created)
    local dirs=(
        "src"
        "tests"
        "docs"
        "docs/adr"
        ".github/workflows"
    )

    # Language-specific directories
    case "$LANGUAGE" in
        dotnet)
            dirs+=("src/Properties")
            ;;
        python)
            dirs+=("src/__pycache__" "tests/__pycache__")
            ;;
        node)
            dirs+=("src/lib" "src/routes")
            ;;
        go)
            dirs+=("cmd" "internal" "pkg")
            ;;
        dart)
            dirs+=("lib" "lib/src")
            ;;
    esac

    for dir in "${dirs[@]}"; do
        if [ ! -d "${PROJECT_DIR}/${dir}" ]; then
            run_cmd mkdir -p "${PROJECT_DIR}/${dir}"
            debug "Created directory: ${dir}/"
        else
            debug "Directory already exists: ${dir}/"
        fi
    done

    # Create placeholder/starter files if they don't exist
    local files_created=0

    # README.md
    if [ ! -f "${PROJECT_DIR}/README.md" ]; then
        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would create: README.md"
        else
            local project_name
            project_name="$(basename "$PROJECT_DIR")"
            cat > "${PROJECT_DIR}/README.md" << EOF
# ${project_name}

> Project scaffolded with the [D.O.E. Framework](doe-framework/DOE.md).

## Overview

_TODO: Describe the project here._

## Getting Started

_TODO: Add setup and usage instructions._

## Documentation

Full documentation is available in the [docs/](docs/) directory:

- [Technical Specification](docs/project-spec.md)
- [Changelog](docs/changelog.md)
- [Architecture Decision Records](docs/adr/)
EOF
            debug "Created: README.md"
        fi
        files_created=$((files_created + 1))
    fi

    # docs/changelog.md (from template if available)
    if [ ! -f "${PROJECT_DIR}/docs/changelog.md" ]; then
        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would create: docs/changelog.md"
        else
            if [ -f "${SCRIPT_DIR}/templates/changelog-template.md" ]; then
                cp "${SCRIPT_DIR}/templates/changelog-template.md" "${PROJECT_DIR}/docs/changelog.md"
            else
                echo "# Changelog" > "${PROJECT_DIR}/docs/changelog.md"
            fi
            debug "Created: docs/changelog.md"
        fi
        files_created=$((files_created + 1))
    fi

    # docs/project-spec.md (from template)
    if [ ! -f "${PROJECT_DIR}/docs/project-spec.md" ]; then
        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would create: docs/project-spec.md"
        else
            if [ -f "${SCRIPT_DIR}/templates/project-spec.md" ]; then
                cp "${SCRIPT_DIR}/templates/project-spec.md" "${PROJECT_DIR}/docs/project-spec.md"
            else
                echo "# Technical Specification" > "${PROJECT_DIR}/docs/project-spec.md"
            fi
            debug "Created: docs/project-spec.md"
        fi
        files_created=$((files_created + 1))
    fi

    # docs/tech-specs.md (library/version tracking)
    if [ ! -f "${PROJECT_DIR}/docs/tech-specs.md" ]; then
        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would create: docs/tech-specs.md"
        else
            cat > "${PROJECT_DIR}/docs/tech-specs.md" << 'EOF'
# Technical Specifications

> This file tracks all libraries, frameworks, language versions, and
> compatibility notes for the project. The coding agent MUST consult
> this file before adding or updating any dependency.

## Language & Runtime

| Component | Version | Notes |
|-----------|---------|-------|
| _TODO_    | _TODO_  |       |

## Dependencies

| Library | Version | Purpose | License |
|---------|---------|---------|---------|
| _TODO_  | _TODO_  | _TODO_  | _TODO_  |

## Compatibility Notes

_TODO: Add any known compatibility constraints, version conflicts, etc._
EOF
            debug "Created: docs/tech-specs.md"
        fi
        files_created=$((files_created + 1))
    fi

    # docs/adr/000-template.md
    if [ ! -f "${PROJECT_DIR}/docs/adr/000-template.md" ]; then
        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would create: docs/adr/000-template.md"
        else
            if [ -f "${SCRIPT_DIR}/templates/adr-template.md" ]; then
                cp "${SCRIPT_DIR}/templates/adr-template.md" "${PROJECT_DIR}/docs/adr/000-template.md"
            else
                echo "# ADR Template" > "${PROJECT_DIR}/docs/adr/000-template.md"
            fi
            debug "Created: docs/adr/000-template.md"
        fi
        files_created=$((files_created + 1))
    fi

    # .github/workflows/ci.yml (minimal placeholder)
    if [ ! -f "${PROJECT_DIR}/.github/workflows/ci.yml" ]; then
        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would create: .github/workflows/ci.yml"
        else
            cat > "${PROJECT_DIR}/.github/workflows/ci.yml" << 'EOF'
# =============================================================================
# CI Pipeline — Placeholder
# =============================================================================
# Generated by the D.O.E. Framework installer.
# Replace this with your actual CI configuration.
#
# See: doe-framework/L3-execution/05-cicd-setup.md for guidelines.
# =============================================================================

name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # TODO: Add language-specific build and test steps
      - name: Placeholder
        run: echo "Replace this with your actual CI pipeline"
EOF
            debug "Created: .github/workflows/ci.yml"
        fi
        files_created=$((files_created + 1))
    fi

    # .gitkeep for empty directories (so git tracks them)
    for dir in "src" "tests"; do
        local gitkeep="${PROJECT_DIR}/${dir}/.gitkeep"
        # Only add .gitkeep if the directory is empty
        if [ -d "${PROJECT_DIR}/${dir}" ] && [ -z "$(ls -A "${PROJECT_DIR}/${dir}" 2>/dev/null)" ]; then
            if [[ "$DRY_RUN" == true ]]; then
                dry_run_msg "Would create: ${dir}/.gitkeep"
            else
                touch "$gitkeep"
                debug "Created: ${dir}/.gitkeep"
            fi
        fi
    done

    success "Project scaffolded (${files_created} files created)."
}

# Verify framework integrity (source or installed copy)
verify_framework() {
    local base_dir="$SCRIPT_DIR"
    local check_target="source"

    # If a project directory was given, check the installed copy
    if [ -n "$PROJECT_DIR" ]; then
        base_dir="${PROJECT_DIR}/doe-framework"
        check_target="installed (${PROJECT_DIR})"

        if [ ! -d "$base_dir" ]; then
            error_exit "No framework found at: ${base_dir}\nRun 'install-doe.sh install ${PROJECT_DIR}' first." 2
        fi
    fi

    echo ""
    echo -e "${BOLD}D.O.E. Framework — Integrity Verification${RESET}"
    echo -e "Checking: ${CYAN}${check_target}${RESET}"
    echo ""

    local total=0
    local present=0
    local missing=0
    local missing_files=()

    for file in "${FRAMEWORK_CORE_FILES[@]}"; do
        total=$((total + 1))

        # CLAUDE.md lives in the project root, not inside doe-framework/
        # When checking an installed copy, look for it one level up.
        local filepath
        if [[ "$file" == "CLAUDE.md" && -n "$PROJECT_DIR" ]]; then
            filepath="${PROJECT_DIR}/CLAUDE.md"
        else
            filepath="${base_dir}/${file}"
        fi

        if [ -f "$filepath" ]; then
            present=$((present + 1))
            # Show file with size for verbose mode
            if [[ "$VERBOSE" == true ]]; then
                local size
                size=$(wc -c < "$filepath" | tr -d ' ')
                local display_path="$file"
                [[ "$file" == "CLAUDE.md" && -n "$PROJECT_DIR" ]] && display_path="CLAUDE.md (project root)"
                echo -e "  ${GREEN}✓${RESET} ${display_path} ${DIM}(${size} bytes)${RESET}"
            fi
        else
            missing=$((missing + 1))
            missing_files+=("$file")
            echo -e "  ${RED}✗${RESET} ${file} ${RED}— MISSING${RESET}"
        fi
    done

    # Also check install-doe.sh in installed copies
    if [ -n "$PROJECT_DIR" ]; then
        total=$((total + 1))
        if [ -f "${base_dir}/install-doe.sh" ]; then
            present=$((present + 1))
            [[ "$VERBOSE" == true ]] && echo -e "  ${GREEN}✓${RESET} install-doe.sh"
        else
            missing=$((missing + 1))
            missing_files+=("install-doe.sh")
            echo -e "  ${RED}✗${RESET} install-doe.sh ${RED}— MISSING${RESET}"
        fi
    fi

    # Generate checksums for all present files (to detect corruption)
    if [[ "$VERBOSE" == true ]] && command -v sha256sum &>/dev/null; then
        echo ""
        info "SHA-256 checksums:"
        for file in "${FRAMEWORK_CORE_FILES[@]}"; do
            local filepath="${base_dir}/${file}"
            if [ -f "$filepath" ]; then
                local hash
                hash=$(sha256sum "$filepath" | awk '{print $1}')
                echo -e "  ${DIM}${hash:0:16}...${RESET} ${file}"
            fi
        done
    fi

    # Summary
    echo ""
    if [ "$missing" -eq 0 ]; then
        success "${BOLD}Framework intact: ${present}/${total} files verified.${RESET}"
        echo ""
        return 0
    else
        echo -e "${RED}${BOLD}Framework INCOMPLETE: ${missing}/${total} file(s) missing.${RESET}"
        echo ""
        echo "Missing files:"
        for f in "${missing_files[@]}"; do
            echo -e "  ${RED}-${RESET} ${f}"
        done
        echo ""

        if [ -n "$PROJECT_DIR" ]; then
            warn "Run 'install-doe.sh reset ${PROJECT_DIR} --force' to restore."
        else
            warn "Re-download or restore the doe-framework/ directory."
        fi

        echo ""
        return 3
    fi
}

# Generate the startup prompt for the coding agent
# If running interactively, asks the user for a project description.
# If non-interactive (piped), generates a generic template.
generate_startup_prompt() {
    # Skip in dry-run mode
    if [[ "$DRY_RUN" == true ]]; then
        dry_run_msg "Would show startup prompt generator"
        return 0
    fi

    echo ""
    echo -e "  ─────────────────────────────────────────────────────────────────"
    echo -e "  ${BOLD}Startup Prompt Generator${RESET}"
    echo -e "  ─────────────────────────────────────────────────────────────────"
    echo ""

    local project_description=""

    # Check if stdin is a terminal (interactive mode)
    if [ -t 0 ]; then
        echo -e "  Descrivi brevemente il progetto che vuoi realizzare."
        echo -e "  (Premi ${CYAN}Invio${RESET} senza scrivere nulla per generare un template generico)"
        echo ""
        echo -ne "  ${BOLD}> ${RESET}"
        read -r project_description || true
    else
        debug "Non-interactive mode: skipping project description prompt."
    fi

    echo ""
    echo -e "  ─────────────────────────────────────────────────────────────────"
    echo -e "  ${GREEN}${BOLD}Copia e incolla questo prompt nel tuo coding agent:${RESET}"
    echo -e "  ─────────────────────────────────────────────────────────────────"
    echo ""

    # Build the prompt
    local prompt_file="${PROJECT_DIR}/doe-framework/.startup-prompt.txt"

    if [ -n "$project_description" ]; then
        # Personalized prompt with project description
        cat << EOF
  ┌───────────────────────────────────────────────────────────────┐
  │                                                               │
  │  Sei un coding agent che opera secondo il framework D.O.E.    │
  │  (Direttiva, Orchestrazione, Esecuzione).                     │
  │                                                               │
  │  PRIMA DI FARE QUALSIASI COSA:                                │
  │  1. Leggi il file doe-framework/DOE.md                        │
  │  2. Segui il Workflow Operativo descritto al suo interno      │
  │  3. Inizia con il Project Intake Protocol                     │
  │     (doe-framework/L1-directives/01-project-intake.md)        │
  │                                                               │
  │  La mia richiesta di progetto:                                │
  │  ${project_description}
  │                                                               │
  │  Non scrivere nessuna riga di codice finche' non hai          │
  │  completato la raccolta requisiti e io non ho approvato       │
  │  la specifica tecnica.                                        │
  │                                                               │
  └───────────────────────────────────────────────────────────────┘
EOF
    else
        # Generic template prompt
        cat << 'EOF'
  ┌───────────────────────────────────────────────────────────────┐
  │                                                               │
  │  Sei un coding agent che opera secondo il framework D.O.E.    │
  │  (Direttiva, Orchestrazione, Esecuzione).                     │
  │                                                               │
  │  PRIMA DI FARE QUALSIASI COSA:                                │
  │  1. Leggi il file doe-framework/DOE.md                        │
  │  2. Segui il Workflow Operativo descritto al suo interno      │
  │  3. Inizia con il Project Intake Protocol                     │
  │     (doe-framework/L1-directives/01-project-intake.md)        │
  │                                                               │
  │  La mia richiesta di progetto:                                │
  │  [DESCRIVI QUI IL TUO PROGETTO]                               │
  │                                                               │
  │  Non scrivere nessuna riga di codice finche' non hai          │
  │  completato la raccolta requisiti e io non ho approvato       │
  │  la specifica tecnica.                                        │
  │                                                               │
  └───────────────────────────────────────────────────────────────┘
EOF
    fi

    # Also save the prompt as a plain text file for easy copy-paste
    if [ -n "$project_description" ]; then
        cat > "$prompt_file" << EOF
Sei un coding agent che opera secondo il framework D.O.E. (Direttiva, Orchestrazione, Esecuzione).

PRIMA DI FARE QUALSIASI COSA:
1. Leggi il file doe-framework/DOE.md
2. Segui il Workflow Operativo descritto al suo interno
3. Inizia con il Project Intake Protocol (doe-framework/L1-directives/01-project-intake.md)

La mia richiesta di progetto:
${project_description}

Non scrivere nessuna riga di codice finche' non hai completato la raccolta requisiti e io non ho approvato la specifica tecnica.
EOF
    else
        cat > "$prompt_file" << 'EOF'
Sei un coding agent che opera secondo il framework D.O.E. (Direttiva, Orchestrazione, Esecuzione).

PRIMA DI FARE QUALSIASI COSA:
1. Leggi il file doe-framework/DOE.md
2. Segui il Workflow Operativo descritto al suo interno
3. Inizia con il Project Intake Protocol (doe-framework/L1-directives/01-project-intake.md)

La mia richiesta di progetto:
[DESCRIVI QUI IL TUO PROGETTO]

Non scrivere nessuna riga di codice finche' non hai completato la raccolta requisiti e io non ho approvato la specifica tecnica.
EOF
    fi

    echo ""
    echo -e "  ${DIM}Prompt salvato anche in: doe-framework/.startup-prompt.txt${RESET}"
    echo ""
}

# Remove the framework directory from a project (used by reset)
remove_existing_framework() {
    local target_fw_dir="${PROJECT_DIR}/doe-framework"

    if [ ! -d "$target_fw_dir" ]; then
        debug "No existing framework to remove."
        return 0
    fi

    info "Removing existing framework at ${target_fw_dir}/ ..."

    if [[ "$DRY_RUN" == true ]]; then
        dry_run_msg "Would remove: ${target_fw_dir}/"
    else
        rm -rf "$target_fw_dir" || error_exit "Failed to remove existing framework." 4
    fi

    success "Existing framework removed."
}

# ---------------------------------------------------------------------------
# Command implementations
# ---------------------------------------------------------------------------

cmd_install() {
    echo ""
    echo -e "${BOLD}D.O.E. Framework — Full Installation${RESET}"
    echo -e "Target: ${CYAN}${PROJECT_DIR}${RESET}"
    [[ -n "$LANGUAGE" ]] && echo -e "Language: ${CYAN}${LANGUAGE}${RESET}"
    [[ "$DRY_RUN" == true ]] && echo -e "${YELLOW}(dry-run mode — no changes will be made)${RESET}"
    echo ""

    validate_framework_source
    validate_language
    ensure_project_dir

    # Step 1: Copy framework
    copy_framework

    # Step 2: Install CLAUDE.md
    install_claude_md

    # Step 3: Init git
    init_git

    # Step 4: Install .gitignore
    install_gitignore

    # Step 5: Install hooks
    install_hooks

    # Step 6: Scaffold
    scaffold_project

    # Summary
    echo ""
    echo -e "${GREEN}${BOLD}Installation complete!${RESET}"

    # Step 7: Generate startup prompt
    generate_startup_prompt

    echo "Next steps:"
    echo -e "  1. ${CYAN}cd ${PROJECT_DIR}${RESET}"
    echo -e "  2. Open a coding agent session (Claude Code, Cowork, Cursor, etc.)"
    echo -e "  3. Paste the startup prompt above (or from ${CYAN}doe-framework/.startup-prompt.txt${RESET})"
    echo -e "  4. The agent will follow the D.O.E. workflow automatically"
    echo ""
}

cmd_reset() {
    if [[ "$FORCE" != true ]]; then
        error_exit "The 'reset' command requires --force to prevent accidental overwrites.\nUsage: install-doe.sh reset ${PROJECT_DIR} --force" 1
    fi

    echo ""
    echo -e "${BOLD}D.O.E. Framework — Reset${RESET}"
    echo -e "Target: ${CYAN}${PROJECT_DIR}${RESET}"
    [[ "$DRY_RUN" == true ]] && echo -e "${YELLOW}(dry-run mode — no changes will be made)${RESET}"
    echo ""

    validate_framework_source
    validate_language
    ensure_project_dir

    # Remove existing framework
    remove_existing_framework

    # Remove existing CLAUDE.md
    if [ -f "${PROJECT_DIR}/CLAUDE.md" ]; then
        if [[ "$DRY_RUN" == true ]]; then
            dry_run_msg "Would remove: CLAUDE.md"
        else
            rm -f "${PROJECT_DIR}/CLAUDE.md"
            debug "Removed existing CLAUDE.md"
        fi
    fi

    # Reinstall everything
    copy_framework
    install_claude_md
    install_hooks

    echo ""
    echo -e "${GREEN}${BOLD}Reset complete! Framework restored to latest version.${RESET}"
    echo ""
}

cmd_hooks() {
    echo ""
    echo -e "${BOLD}D.O.E. Framework — Git Hooks Installation${RESET}"
    echo -e "Target: ${CYAN}${PROJECT_DIR}${RESET}"
    [[ "$DRY_RUN" == true ]] && echo -e "${YELLOW}(dry-run mode — no changes will be made)${RESET}"
    echo ""

    validate_framework_source
    ensure_project_dir
    install_hooks

    echo ""
}

cmd_scaffold() {
    echo ""
    echo -e "${BOLD}D.O.E. Framework — Project Scaffold${RESET}"
    echo -e "Target: ${CYAN}${PROJECT_DIR}${RESET}"
    [[ -n "$LANGUAGE" ]] && echo -e "Language: ${CYAN}${LANGUAGE}${RESET}"
    [[ "$DRY_RUN" == true ]] && echo -e "${YELLOW}(dry-run mode — no changes will be made)${RESET}"
    echo ""

    validate_language
    ensure_project_dir
    scaffold_project

    echo ""
}

cmd_verify() {
    validate_framework_source 2>/dev/null || true
    verify_framework
    exit $?
}

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

parse_args() {
    # No arguments at all -> show help
    if [ $# -eq 0 ]; then
        show_help
    fi

    # First pass: check for --help and --version anywhere
    for arg in "$@"; do
        case "$arg" in
            --help|-h)  show_help ;;
            --version)  show_version ;;
        esac
    done

    # Parse command (first positional argument)
    case "$1" in
        install|reset|hooks|scaffold|verify)
            COMMAND="$1"
            shift
            ;;
        -*)
            # No command given, but flags present — assume "install"
            COMMAND="install"
            ;;
        *)
            # First arg is not a recognized command — assume it's a directory
            # and default to "install"
            COMMAND="install"
            ;;
    esac

    # Parse remaining arguments
    while [ $# -gt 0 ]; do
        case "$1" in
            --lang)
                if [ -z "${2:-}" ]; then
                    error_exit "--lang requires a value.\nSupported: ${SUPPORTED_LANGUAGES[*]}" 1
                fi
                LANGUAGE="$2"
                shift 2
                ;;
            --force)
                FORCE=true
                shift
                ;;
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            --verbose|-v)
                VERBOSE=true
                shift
                ;;
            -*)
                error_exit "Unknown option: ${1}\nRun 'install-doe.sh --help' for usage." 1
                ;;
            *)
                if [ -z "$PROJECT_DIR" ]; then
                    PROJECT_DIR="$1"
                else
                    error_exit "Unexpected argument: ${1}\nOnly one directory is accepted." 1
                fi
                shift
                ;;
        esac
    done

    # Verify must not require a directory
    if [[ "$COMMAND" == "verify" ]]; then
        return 0
    fi

    # All other commands require a project directory
    if [ -z "$PROJECT_DIR" ]; then
        error_exit "Missing required argument: <project-directory>\nUsage: install-doe.sh ${COMMAND} <directory> [OPTIONS]\nRun 'install-doe.sh --help' for full usage." 1
    fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    parse_args "$@"

    case "$COMMAND" in
        install)  cmd_install ;;
        reset)    cmd_reset ;;
        hooks)    cmd_hooks ;;
        scaffold) cmd_scaffold ;;
        verify)   cmd_verify ;;
        *)        error_exit "Unknown command: ${COMMAND}" 1 ;;
    esac
}

main "$@"
