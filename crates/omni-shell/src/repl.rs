//! Read-eval-print loop (REPL).
//!
//! This module drives the interactive shell session. It provides two public
//! entry points:
//!
//! - [`format_prompt`]: formats the shell prompt string by expanding `PS1`
//!   escape sequences.
//! - [`process_line`]: runs a single input line through the full shell
//!   pipeline — tokenise → expand aliases → parse → execute.
//!
//! The [`Shell`] struct bundles the environment, line editor, and current
//! working directory. In a live session the REPL owner calls [`process_line`]
//! for every line obtained from the line editor, then uses the returned exit
//! code to update `$?`.
//!
//! ## Pipeline
//!
//! ```text
//! raw &str
//!   ──► lexer::tokenize           →  Vec<Token>
//!   ──► parser::parse             →  CommandList (AST)
//!   ──► executor::execute_command_list  →  i32 (exit code)
//!       (builtins run in-process; env/glob expansion happens inside executor)
//! ```
//!
//! ## Comments and blank lines
//!
//! Lines that are empty (after trimming) or that begin with `#` are treated as
//! no-ops and return exit code `0` immediately.

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

use crate::command;
use crate::env::ShellEnv;
use crate::executor::{self, ExecContext};
use crate::glob::FsQuery;
use crate::lexer;
use crate::line_editor::LineEditor;
use crate::parser;

// ── format_prompt ─────────────────────────────────────────────────────────────

/// Format the shell prompt string.
///
/// If the `PS1` environment variable is set, its value is used as the template
/// with the following escape sequences expanded:
///
/// | Sequence | Expansion |
/// |----------|-----------|
/// | `\u` | Value of `$USER` (or `?` if unset). |
/// | `\h` | Value of `$HOSTNAME` (or `omni` if unset). |
/// | `\w` | Current working directory (`cwd`). |
/// | `\$` | `$` (allows `PS1` to include a literal dollar sign). |
///
/// If `PS1` is not set, the default prompt format is used:
/// `<USER>@<HOSTNAME>:<cwd>$ `.
///
/// # Examples
///
/// ```rust
/// use omni_shell::env::ShellEnv;
/// use omni_shell::repl::format_prompt;
///
/// let env = ShellEnv::new();
/// let prompt = format_prompt(&env, "/home/root");
/// assert!(prompt.contains("root@"));
/// assert!(prompt.contains("/home/root"));
/// assert!(prompt.ends_with("$ "));
/// ```
pub fn format_prompt(env: &ShellEnv, cwd: &str) -> String {
    env.get("PS1").map_or_else(
        || {
            format!(
                "{}@{}:{}$ ",
                env.get("USER").unwrap_or("root"),
                env.get("HOSTNAME").unwrap_or("omni"),
                cwd
            )
        },
        |ps1| {
            ps1.replace("\\u", env.get("USER").unwrap_or("?"))
                .replace("\\h", env.get("HOSTNAME").unwrap_or("omni"))
                .replace("\\w", cwd)
                .replace("\\$", "$")
        },
    )
}

// ── process_line ──────────────────────────────────────────────────────────────

/// Process a single input line through the complete shell pipeline.
///
/// Steps performed:
/// 1. Trim leading and trailing whitespace.
/// 2. Skip empty lines and comments (`#`-prefixed) — return `0`.
/// 3. Tokenise via [`lexer::tokenize`]; syntax errors return `1`.
/// 4. Parse via [`parser::parse`]; parse errors return `1`.
/// 5. Execute via [`executor::execute_command_list`].
/// 6. Flush captured output to stdout via `print!`.
/// 7. Update `*cwd` from the execution context (the `cd` builtin may have
///    changed it).
///
/// # Returns
///
/// The exit code of the last executed pipeline, or `0` for blank/comment
/// lines, or `1` on tokenisation/parse failure.
///
/// # Examples
///
/// ```rust
/// use omni_shell::env::ShellEnv;
/// use omni_shell::repl::process_line;
/// use omni_shell::glob::FsQuery;
///
/// struct EmptyFs;
/// impl FsQuery for EmptyFs {
///     fn list_dir(&self, _: &str) -> Result<Vec<String>, String> { Ok(vec![]) }
/// }
///
/// let mut env = ShellEnv::new();
/// let mut cwd = "/".to_string();
/// let code = process_line("echo hello", &mut env, &mut cwd, &EmptyFs);
/// assert_eq!(code, 0);
/// ```
pub fn process_line(input: &str, env: &mut ShellEnv, cwd: &mut String, fs: &dyn FsQuery) -> i32 {
    let trimmed = input.trim();

    // Empty lines and comments are no-ops.
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return 0;
    }

    // Tokenise.
    let tokens = match lexer::tokenize(trimmed) {
        Ok(t) => t,
        Err(e) => {
            // Tracing is only available when the `std` feature is enabled.
            // On bare-metal targets the REPL caller handles error reporting.
            #[cfg(feature = "std")]
            tracing::warn!(error = %e, "omni-shell: syntax error");
            let _ = e;
            return 1;
        }
    };
    if tokens.is_empty() {
        return 0;
    }

    // Parse.
    let ast = match parser::parse(&tokens) {
        Ok(a) => a,
        Err(e) => {
            #[cfg(feature = "std")]
            tracing::warn!(error = %e, "omni-shell: parse error");
            let _ = e;
            return 1;
        }
    };
    if ast.entries.is_empty() {
        return 0;
    }

    // Classify intent before execution so the label can be prepended to output
    // and the class is available for the audit record.
    let intent = crate::intent::classify_intent(trimmed);

    // Execute.
    let builtins = command::register_builtins();
    let mut ctx = ExecContext {
        env,
        last_exit_code: 0,
        cwd: cwd.clone(),
        fs,
        output: Vec::new(),
        audit_log: crate::audit::AuditLog::new(),
    };
    let code = executor::execute_command_list(&ast, &mut ctx, &builtins);

    // Propagate cwd changes (the `cd` builtin updates ctx.cwd).
    *cwd = ctx.cwd;

    // Flush captured output to stdout when running on a host with std.
    // On bare-metal targets the caller reads `ctx.output` directly and
    // dispatches it via the kernel console/syscall layer.
    #[cfg(feature = "std")]
    if !ctx.output.is_empty() {
        // When OMNI_AGENT=1 is set in the shell environment, prepend the
        // intent label so the user can see which agent handled the request.
        if ctx.env.get("OMNI_AGENT") == Some("1") {
            print!(
                "{} {}",
                crate::intent::agent_label(intent),
                String::from_utf8_lossy(&ctx.output)
            );
        } else {
            print!("{}", String::from_utf8_lossy(&ctx.output));
        }
    }

    // When OMNI_MODE=high-risk is set and the intent is sensitive, emit a
    // structured warning via tracing so the operator is aware of potentially
    // dangerous operations before execution completes.
    #[cfg(feature = "std")]
    if ctx.env.get("OMNI_MODE") == Some("high-risk") {
        match intent {
            crate::intent::IntentClass::Administration | crate::intent::IntentClass::Security => {
                tracing::warn!(
                    intent = crate::intent::agent_label(intent),
                    "high-risk mode: sensitive intent detected — review before proceeding"
                );
            }
            _ => {}
        }
    }

    // Record in audit log. Timestamp is 0 in Phase 1 (no HAL clock yet).
    ctx.audit_log.record(trimmed.into(), intent, code, 0);

    code
}

// ── Shell ─────────────────────────────────────────────────────────────────────

/// The interactive shell instance.
///
/// Bundles together:
/// - The runtime environment ([`ShellEnv`]).
/// - The interactive line editor ([`LineEditor`]).
/// - The current working directory.
///
/// # Examples
///
/// ```rust
/// use omni_shell::repl::Shell;
///
/// let shell = Shell::new();
/// assert_eq!(shell.cwd, "/");
/// ```
pub struct Shell {
    /// The shell's variable, alias, and export environment.
    pub env: ShellEnv,
    /// The interactive line editor (history, key bindings, rendering).
    pub editor: LineEditor,
    /// Current working directory; kept in sync with `$PWD`.
    pub cwd: String,
}

impl Shell {
    /// Create a new shell with default environment, a fresh line editor, and
    /// the root directory as the initial working directory.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use omni_shell::repl::Shell;
    ///
    /// let shell = Shell::new();
    /// assert_eq!(shell.cwd, "/");
    /// assert_eq!(shell.env.get("HOME"), Some("/"));
    /// ```
    pub fn new() -> Self {
        Self {
            env: ShellEnv::new(),
            editor: LineEditor::new(),
            cwd: String::from("/"),
        }
    }
}

impl Default for Shell {
    /// Create a default shell identical to [`Shell::new`].
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glob::FsQuery;

    // ── Mock filesystem ───────────────────────────────────────────────────

    struct EmptyFs;
    impl FsQuery for EmptyFs {
        fn list_dir(&self, _path: &str) -> Result<Vec<String>, String> {
            Ok(vec![])
        }
    }

    // ── format_prompt ─────────────────────────────────────────────────────

    #[test]
    fn format_prompt_default_format() {
        let env = ShellEnv::new(); // USER=root, no HOSTNAME, no PS1
        let prompt = format_prompt(&env, "/home/root");
        assert!(prompt.starts_with("root@"), "prompt was: {prompt:?}");
        assert!(prompt.contains("/home/root"), "prompt was: {prompt:?}");
        assert!(prompt.ends_with("$ "), "prompt was: {prompt:?}");
    }

    #[test]
    fn format_prompt_with_ps1_variable() {
        let mut env = ShellEnv::new();
        env.set("PS1", "\\u@\\h:\\w\\$ ");
        env.set("USER", "alice");
        env.set("HOSTNAME", "box");
        let prompt = format_prompt(&env, "/tmp");
        assert_eq!(prompt, "alice@box:/tmp$ ");
    }

    #[test]
    fn format_prompt_ps1_partial_escapes() {
        let mut env = ShellEnv::new();
        env.set("PS1", "[\\w]\\$ ");
        let prompt = format_prompt(&env, "/srv");
        assert_eq!(prompt, "[/srv]$ ");
    }

    // ── process_line ──────────────────────────────────────────────────────

    #[test]
    fn process_line_empty_returns_zero() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        let code = process_line("", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
    }

    #[test]
    fn process_line_whitespace_only_returns_zero() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        let code = process_line("   \t  ", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
    }

    #[test]
    fn process_line_comment_returns_zero() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        let code = process_line("# this is a comment", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
    }

    #[test]
    fn process_line_echo_returns_zero() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        let code = process_line("echo hello", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
    }

    #[test]
    fn process_line_true_returns_zero() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        let code = process_line("true", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
    }

    #[test]
    fn process_line_false_returns_one() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        let code = process_line("false", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 1);
    }

    #[test]
    fn process_line_cd_changes_cwd() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        let code = process_line("cd /tmp", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
        assert_eq!(cwd, "/tmp");
    }

    #[test]
    fn process_line_unknown_command_returns_127() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        let code = process_line("totally_unknown_cmd_xyz", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 127);
    }

    #[test]
    fn process_line_with_variable_expansion() {
        let mut env = ShellEnv::new();
        env.set("MYVAR", "expanded");
        let mut cwd = "/".to_string();
        // echo $MYVAR — the value is expanded before execution.
        let code = process_line("echo $MYVAR", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
    }

    #[test]
    fn process_line_and_chaining() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        // true && true should return 0.
        let code = process_line("true && true", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
    }

    #[test]
    fn process_line_or_chaining_after_failure() {
        let mut env = ShellEnv::new();
        let mut cwd = "/".to_string();
        // false || true should return 0.
        let code = process_line("false || true", &mut env, &mut cwd, &EmptyFs);
        assert_eq!(code, 0);
    }

    // ── Shell struct ──────────────────────────────────────────────────────

    #[test]
    fn shell_new_has_root_cwd() {
        let shell = Shell::new();
        assert_eq!(shell.cwd, "/");
    }

    #[test]
    fn shell_new_has_default_home() {
        let shell = Shell::new();
        assert_eq!(shell.env.get("HOME"), Some("/"));
    }

    #[test]
    fn shell_default_equals_new() {
        let a = Shell::new();
        let b = Shell::default();
        assert_eq!(a.cwd, b.cwd);
        assert_eq!(a.env.get("USER"), b.env.get("USER"));
    }
}
