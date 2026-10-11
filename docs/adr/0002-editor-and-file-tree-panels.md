# ADR-0002: Editor and file tree panels

## Status

Accepted — 2026-10-11.

## Context

The blueprint (`docs/blueprint.md:353,373`) schedules a code editor for Phase 3 and forbids editors in Phase 1. The owner has pulled a native editor and file manager forward. Design: `docs/superpowers/specs/2026-10-11-code-editor-design.md`.

## Decision

- Two new registered panel kinds, `PanelKind::Editor` and `PanelKind::FileTree`, both referencing `ResourceRef::Document(DocumentId)`.
- The file or folder path is stored per panel in a new nullable column, `panels.path`, added by schema migration v4. This satisfies ADR-0001's rule that schema changes need a migration plus an ADR addendum.
- The UI keeps terminals and documents in separate maps so terminal code paths are untouched.
- Later steps add these dependencies: `ropey`, `tree-sitter` and built-in grammars, `regex-cursor`, `ignore`, `notify`, `nucleo`, `trash`. Each is justified in the design spec's dependency table.

## Consequences

- A database written by this version can't be opened by older builds (downgrade is rejected, as before).
- Editors and file trees never spawn processes. Restore re-reads the file from disk, and unsaved edits are not persisted.
