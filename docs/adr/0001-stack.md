# ADR-0001: Implementation stack

## Status

Accepted — 2026-10-10. Pending Gate 0 spike validation.

## Context

This project is a native Rust terminal workspace and AI agent control center targeting macOS on Apple Silicon first, with a platform-neutral core and a Linux smoke milestone before Phase 1 signoff (per NDW-PRD-001 §1). The intent is a local-first, terminal-first, keyboard-first app where typing, scrolling, colors and cursor behavior are never sacrificed for dashboard chrome. The implementation rule forbids forking Wave, Electron, Ghostty, Zellij, or any other terminal application — only library reuse is permitted.

The candidate stack reuses proven primitives instead of rebuilding a VT parser, font rasterizer, PTY abstraction, or SQL engine. Each choice is being evaluated against three questions: can it ship in Phase 1 on macOS, does it keep the domain independent of the UI framework, and does it have a credible Linux path. GPUI is the desktop chrome and rendering layer; `alacritty_terminal` owns VT/screen state; `portable-pty` provides subprocess PTYs; SQLite holds all persisted metadata; TOML is the user-facing config format. Final crate versions are pinned in the workspace `Cargo.toml` only after the Gate 0 spikes succeed; an alternative UI decision would be raised as a separate ADR.

## Decision drivers

- **Terminal fidelity over chrome (FR-01, FR-06).** The chosen VT engine must match Alacritty's grid behaviour for vi, htop, ssh, bracketed paste, alternate screen, true color and wide glyphs.
- **Domain isolation (FR-07, §6).** Workspace, Tab, LayoutNode, Panel and Agent types must compile without UI/framework/storage imports; `domain/` is the only crate allowed at the bottom of the dependency graph.
- **Process lifecycle safety (FR-01, §9).** Subprocess ownership must be explicit so quit/crash never leaks agents; argv-vector execution, recorded PIDs, and supervised termination are required.
- **Single-instance persistence (FR-07).** Atomic SQLite transactions, version-gated migrations and a single-instance lock; no auto-execution on restore.
- **Auditability and log hygiene (§13).** Structured traces across PTY, VT, DB and async seams; secrets and prompt contents must be redacted by default.
- **Cross-platform headroom (§1, US-10).** Linux is a Phase 1 exit goal; every platform-dependent choice must have a documented path, not just a hope.

## Decision

| Layer | Choice | Rationale | Version strategy |
|---|---|---|---|
| Language | Rust stable | Native binary, predictable perf, strong domain types, ergonomic FFI | Track stable; pin in `rust-toolchain.toml` |
| UI framework | GPUI (Zed) | GPU-rendered native chrome, immediate-mode ergonomics, mature on macOS | Pin after Gate 0 spike A; ADR-0002 if rejected |
| Font rasterization | GPUI bundled (cosmic-text) | Ships with GPUI, covers monospace + emoji fallback, satisfies FR-06 token needs | Track GPUI's pinned version |
| VT state | `alacritty_terminal` | Battle-tested grid/parser, used by Alacritty and Zed terminal | Pin after spike B |
| PTY | `portable-pty` | Cross-platform PTY façade; macOS/Linux coverage without bespoke forks | Pin after spike C |
| Persistence | SQLite via `rusqlite` | Embedded, transactional, migration-friendly metadata store | Pin after spike D |
| Config | TOML via `toml` | Human-editable, versioned, plays well with serde | Pin alongside persistence spike |
| Serialization | `serde` + `serde_json` | Shared derives for domain types, IPC payloads, export/import | Pin with persistence spike |
| Error model | `thiserror` + `anyhow` | Typed library errors via `thiserror`; `anyhow` only at the binary edge | Pin with persistence spike |
| Async runtime | Tokio | Used by `tracing-subscriber` and future IPC; PTY/DB remain thread-pooled, not on the runtime | Pin with workspace bootstrap |
| Observability | `tracing` + `tracing-subscriber` | Structured spans across async/PTY/DB seams; satisfies FR-13 diagnostics | Pin with workspace bootstrap |
| IDs | UUID v7 | Time-ordered, stable, indexable; preferred over random v4 | Use `uuid` crate, v7 feature |

## Gate 0 spike mapping

| Spike | Validates | Gate to next workstream |
|---|---|---|
| A — GPUI launch | macOS cold start, window/chrome render path, Linux smoke feasibility, IME and text input | UI/UX workstream may scaffold |
| B — PTY → VT → render | `portable-pty` → `alacritty_terminal` → GPUI terminal renderer with vi, htop, ssh, Unicode, truecolor, resize, clipboard | Terminal core may harden |
| C — Process lifecycle | One child startup/stop, exit capture, cancellation, signal propagation | Agent workstream may scaffold |
| D — Baseline profile | Clean-app memory, cold-start time on reference Apple Silicon hardware | Persistence decisions may lock |

If any spike fails its exit criteria, the affected row is reopened as ADR-0002 (UI alternative) or ADR-0003 (terminal alternative) before any feature workstream extends beyond Gate 0.

## Consequences

- **Language (Rust stable).** New contributors need Rust fluency; rendering font-engine and platform glue still need manual work. Mitigation: keep `domain/` portable enough that platform-specific code is isolated to `platform/` and `ui/`.
- **UI framework (GPUI).** macOS-first posture; Linux and Windows are not first-class. We accept that risk in exchange for the rendering quality and Zed ecosystem alignment, and Gate 0 spike A must validate Linux feasibility before any shared-component code lands. If GPUI cannot be exercised on Linux, ADR-0002 opens the path to a fallback framework.
- **Font rasterization (GPUI bundled).** We accept whatever font stack GPUI ships with in Phase 1; Phase 1 stretch may add explicit `cosmic-text` configuration if FR-06 typography needs outgrow defaults. We do not roll our own font engine.
- **VT state (`alacritty_terminal`).** History of API churn between minor versions. We isolate it behind `terminal-core` adapters so version bumps do not leak into the domain or UI crates; a major bump triggers an ADR addendum. The `EventLoop` API is consumed via a thin wrapper that returns our own event enum.
- **PTY (`portable-pty`).** Process-group behavior differs between macOS and Linux (job control, signal propagation). We will require an explicit supervisor in `terminal-core` that records child ownership and uses argv-vector execution; tests must run on at least one Linux distro before Phase 1 exit.
- **Persistence (`rusqlite`).** Synchronous API. PTY and DB calls must stay off the UI thread; we will not adopt an async wrapper in P0 to avoid an unevaluated dependency surface. WAL mode and a single writer connection are mandated; migrations are versioned and reject downgrade.
- **Config (TOML) and serialization (serde/serde_json).** Two format surfaces (TOML for users, JSON for IPC/export). Schema validation lives in the domain layer; failed validation is recoverable, not fatal, and presents a recoverable warning.
- **Error model (`thiserror` + `anyhow`).** Library crates return typed errors; the binary wraps them at the edge. User-facing error messages never leak paths or PIDs without redaction.
- **Async runtime (Tokio).** Adoption is incremental — only the spans that need it. The UI thread never awaits on Tokio futures; PTY reads and DB writes use dedicated threads.
- **Observability (`tracing`).** A single subscriber owns formatting and sink routing (file + optional stderr). Spans are emitted from PTY read, VT update, layout mutation, DB transaction, and command dispatch seams. Prompts, command lines, agent output and env values are redacted by default.
- **IDs (UUID v7).** Time-ordered strings at the persistence boundary. Sortable in SQLite by `text`, indexable, and stable across restart. Random v4 IDs are rejected by lint rules in `domain/`.
- **No forks.** We are deliberately not forking Wave, Electron, Ghostty, or Zellij, per the blueprint's implementation rule. Reuse is at the library API level only; license review precedes any code copy.
- **Domain isolation.** Domain types in `domain/` depend on no UI/framework/storage crate. UI depends only on service interfaces. This constrains future contributors against convenient but boundary-crossing shortcuts.
- **Schema stability.** The ID format and the core schema tables (`workspaces`, `tabs`, `panels`, `layouts`, `agents`, `app_settings`, `schema_migrations`) are fixed at MVP. Any change requires a migration plus an ADR addendum.

## References

- Blueprint §1 — Executive decision and stack baseline
- Blueprint §6 — Architecture and boundaries (dependency direction)
- Blueprint §7 — Domain model and identifiers
- Blueprint §14 — Repository blueprint and crate layering
- Blueprint §15 — Build plan and Gate 0 spike definitions
- Blueprint §17 — QA strategy
- Blueprint §21 — Agent instructions and ticket sequence
- Blueprint §22 — Reference implementations and licenses
- GPUI: <https://github.com/zed-industries/zed/tree/main/crates/gpui>
- `alacritty_terminal`: <https://docs.rs/alacritty_terminal/>
- `portable-pty`: <https://docs.rs/portable-pty/>
- `rusqlite` / SQLite: <https://www.sqlite.org/docs.html>
- `tracing`: <https://docs.rs/tracing/>
- `serde`: <https://docs.rs/serde/>
- `toml`: <https://docs.rs/toml/>
- `thiserror`: <https://docs.rs/thiserror/>
- `anyhow`: <https://docs.rs/anyhow/>
- `tokio`: <https://docs.rs/tokio/>
- `uuid` (v7): <https://docs.rs/uuid/>
