//! # `omni-shell`
//!
//! System shell for OMNI OS.
//!
//! v1 ships a traditional command-line shell sufficient for development
//! and basic system administration. The long-term vision is an
//! **intent-based shell**: natural language is the primary interface, with
//! the shell lowering user intent into a structured plan that the user
//! previews and approves before execution.
//!
//! ## Status
//!
//! Phase 1 complete — lexer, parser, environment, glob, line editor,
//! tab completion, executor, 15 builtins, 20 external commands, REPL.
//!
//! ## Design rationale
//!
//! - **Plan-then-execute**: AI-generated commands never auto-execute.
//!   The user always sees a plan and approves it.
//! - **Capability-aware**: the shell holds a capability for the user's
//!   session and forwards it to invoked commands.
//! - **Auditable**: every command, plan, and result are logged to the
//!   per-user audit log.
//!
//! ## Pipeline
//!
//! ```text
//! raw &str
//!   ──► [`lexer::tokenize`]      →  Vec<Token>
//!   ──► [`parser::parse`]        →  CommandList AST
//!   ──► [`env::ShellEnv::expand`]→  expanded strings
//!   ──► [`glob::expand_glob`]    →  path-expanded args
//!   ──► [`executor`]             →  process / built-in dispatch
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![doc(html_root_url = "https://docs.omni-os.org/omni-shell")]
#![warn(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unnecessary_wraps,
        clippy::indexing_slicing,
    )
)]

extern crate alloc;

/// Built-in command registry.
pub mod command;
/// External commands (Phase 1: implemented as builtins).
pub mod commands;
/// Tab-completion engine.
pub mod completion;
/// Environment variable resolution and expansion.
pub mod env;
/// Command executor — process spawning and pipeline management.
pub mod executor;
/// Glob (pathname) expansion.
pub mod glob;
/// Lexical tokenisation of raw shell input.
pub mod lexer;
/// Interactive line editor with history and key bindings.
pub mod line_editor;
/// Shell parser — converts the token stream into an AST.
pub mod parser;
/// Read-eval-print loop.
pub mod repl;

/// Per-session audit trail.
pub mod audit;
/// Intent classification — lightweight agent integration.
pub mod intent;
