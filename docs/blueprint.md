# Native Developer Workspace — Product & Engineering Specification

**Document ID:** NDW-PRD-001  
**Version:** 1.0 — implementation handoff  
**Date:** 2026-10-10  
**Status:** Proposed architecture; validate dependencies through spikes  
**Owner:** Product / Engineering  
**Scope:** MVP and Phase 1  
**Implementation rule:** New standalone codebase. DO NOT fork Wave, Electron, Ghostty, Zellij, or any other application.

## 1. Executive decision

Build a **local-first, Rust-native terminal workspace and AI agent control center**. It must feel as responsive as a focused terminal emulator, while making tabs, split panes, named workspaces, and coding agents coherent and easy to operate. The app is designed to become a single developer command center later, but **Phase 1 contains no embedded general-purpose browser, code editor, database UI, cloud console, or third-party plugin runtime**.

Use existing **libraries** rather than building a terminal parser, font rasterizer, PTY abstraction, or SQL engine from scratch. The initial target is **macOS on Apple Silicon**, with a platform-neutral core and a Linux compatibility milestone before Phase 1 signoff. Windows is future scope.

**Recommended baseline:** Rust workspace; GPUI for native GPU-rendered app chrome; `alacritty_terminal` for VT/screen state; `portable-pty` for subprocess PTYs; SQLite for metadata; serde/TOML for configuration. Framework APIs and crate versions must be evaluated and pinned before implementing major features. If GPUI fails the cross-platform/rendering spike, raise an Architecture Decision Record (ADR) rather than silently pivoting.

## 2. Product vision and principles

**North star:** Open one lightweight app, select a project or environment, and immediately see its terminals and AI coding agents. Later, open browser, editor, cloud, and database tools within that same workspace without redesigning the foundation.

**Principles:**
1. Terminal-first: typing, scrolling, copy/paste, colors and cursor behavior are never compromised for dashboard UI.
2. Fast and memory-conscious: lazy resources, bounded scrollback, no embedded Chromium runtime in P0.
3. Keyboard-first, mouse-complete: every core action via palette/shortcut; panes draggable/resizable with pointer.
4. Local-first and private: no account, cloud server, telemetry upload, or embedded secrets in SQLite.
5. Native, but portable: Rust business logic independent of the GPUI layer and OS adapters.
6. Extensible by design, not by implementation: typed Panel ID and capabilities now; executable plugins later.
7. Safe lifecycle: close, detach, restart and terminate are deliberately different actions.
8. No magic detection: process-running is not the same as agent-working; state confidence must be explicit.

## 3. Personas, use cases and non-goals

**Primary user:** One power user who switches among several development projects, local/remote shells, infrastructure operations, and concurrent CLI coding agents on a MacBook and Mac mini. Linux support is desirable; multi-user collaboration is not.

**Core jobs to be done:**
- Open a workspace for a project, pick up its terminal layout, work directory, and agents.
- Run many shells across tabs/panes and switch without losing processes.
- Start multiple Claude Code or Codex CLI sessions in assigned directories and quickly focus one.
- See which agents are running, exited, or need attention without parsing every pane.
- Reorganize terminal panes using keyboard or mouse, then recover layouts after a normal restart.
- Keep the interface consistently GitHub Light, uncluttered, and fast with many panes.

**Not in Phase 1:** writing a browser engine, browser panels, Monaco/LSP editing, database execution GUI, AWS/GCP UI integrations, credential vault, full SSH connection manager, marketplace, remote session daemon, cross-device sync, agent task delegation/orchestration, agent transcript parsing, arbitrary plugin execution, Windows distribution.

**Allowed in Phase 1:** SSH via normal `ssh` in a terminal; AWS/GCP via installed local CLI tools; existing tmux/Zellij as user-launched commands. No special proprietary integrations required.

## 4. Scope: MVP versus Phase 1 exit

### MVP — first daily-usable build

MVP requires: one macOS app window; real PTY-backed terminals; tabs; binary split tree; keyboard navigation; named workspaces; persistence of workspace/tab/layout metadata; global GitHub Light colors; command palette; basic agent launch/list/focus/stop for two configurable CLI adapters; safe close/terminate confirmations; diagnostic logging. It must survive ordinary interactive use, with no claim that shells survive app termination.

### Phase 1 exit — hardened personal daily driver

Add: polished tab/pane reorder and movement, advanced terminal fidelity tests, cross-workspace navigation, layout import/export, configurable key bindings, crash recovery of **metadata**, observable agent states, strong process cleanup and cancellation, Linux build/functional smoke tests, performance benchmarks and regression gates, backup/migration tests, installation packaging, docs, and minimum accessibility/keyboard coverage.

**Explicit deferral:** A detached background multiplexer daemon capable of maintaining interactive shell PTYs after GUI exit is Phase 2. Phase 1 must keep processes alive across workspace switches and pane focus changes *while the app is running*, not across a full quit/crash.

## 5. Functional requirements

### FR-01 Terminal engine — P0

- Spawn user default login shell using a real PTY; support configurable shell path, arguments, working directory and environment overrides.
- Correct input/output, Ctrl-C, Ctrl-D, Alt/Option key behavior, paste, selection, clipboard, resizing and SIGWINCH.
- ANSI/VT colors (16/256/true color), Unicode, wide glyphs, combining characters, alternate screen, cursor shapes, bracketed paste and application key modes where supported by terminal library.
- Configurable monospace font family, size, line height; high-DPI text; cursor blink; scrollback limit default 10,000 lines per pane (tunable).
- Terminal title updates; clickable URLs (with safety confirmation for non-HTTP schemes); terminal bell indicator; search within retained scrollback by Phase 1.
- Never drop PTY output solely because pane is hidden. Rendering of unfocused panes can be throttled/coalesced.
- Avoid accidental shell or child termination on layout operations. A pane closing with a live command prompts for termination unless explicitly suppressed.

### FR-02 Tabs, panes and layout — P0

- Each tab has a stable ID, optional name, ordered position, and a rooted binary split layout.
- Split focused pane right or down; resize using keyboard and mouse; navigate directional focus; maximize/unmaximize without destroying underlying tree; move/close panes.
- Multiple tabs per workspace; create, rename, reorder, close and navigate; display focused pane and process/title context.
- Layout invariants: one leaf references one panel instance; no cycles; minimum pane dimensions; closing one pane collapses its parent split; invalid layouts repaired or rejected on load.
- Tab switching and workspace switching must never restart running PTYs.

### FR-03 Named workspaces — P0

- Create, rename, archive/delete and switch named workspaces; optional root path and display icon/color.
- Workspace contains tabs, layout state, focused tab/pane and persisted app configuration overrides.
- Start with at least three workspaces; allow tens of workspaces without eagerly instantiating their UI.
- On restart: restore workspace metadata, layout and tab order; recreate shells using recorded root paths (if existing). Clearly mark recovered shells as **new sessions**, not restored processes.
- If directory is missing or a migrated layout is invalid, open fallback home directory and present a recoverable warning.

### FR-04 AI agent manager — P0

- Provider-agnostic adapter model for external CLI processes, initially configurable `claude` and `codex` commands. Treat installed CLIs as prerequisites; never embed tokens or bypass provider authentication.
- Launch agent in chosen workspace/root directory in an assigned PTY pane; allow CLI arguments from a validated profile, with safe argument-array execution (no shell string interpolation by default).
- Persistent metadata for agent ID, provider, workspace, associated pane/terminal session, cwd, executable/args template, start/end timestamps and state.
- States: `created`, `starting`, `running`, `exited`, `failed`, `stopping`, `unknown`. `running` = OS process alive, **not** proof it is actively thinking. `needs_attention` is optional and only if supported by a documented adapter signal.
- Agent center: list by workspace/provider/status, launch, focus, stop with confirmation, reveal exit code and recent log/PTY view, display elapsed time.
- User can run ordinary CLI agents manually in any terminal. Such shells are NOT auto-classified as managed agents in MVP.
- No auto-approval of tool calls, autonomous actions, model API abstraction, background scheduling, or programmatic parsing of private agent output in P0.

### FR-05 Command palette and shortcuts — P0

- One command registry with stable string IDs, titles, categories, optional shortcuts, enablement predicates and handlers.
- Fuzzy search palette for actions; keyboard focus handoff back to terminal; support `Escape` to close.
- Default mappings: `Cmd+T` new tab, `Cmd+W` close active tab/pane (context-defined), `Cmd+K` palette, `Cmd+Shift+] / [` tab next/previous, `Cmd+Option+Right/Down` split, `Cmd+Option+Arrow` focus adjacent pane, `Cmd+Shift+M` maximize pane. These are proposed defaults; conflicts to validate on macOS/Linux.
- User keymap configuration with conflict detection and a visual shortcut browser by Phase 1 exit.
- Terminal applications receive keys not captured by the app's explicitly registered shortcuts.

### FR-06 Visual design and settings — P0

- Entire application uses a semantic token-based **GitHub Light** palette by default: canvas `#FFFFFF`, secondary canvas `#F6F8FA`, foreground `#1F2328`, border `#D1D9E0`, accent `#0969DA`.
- Terminals get a matching 16-color ANSI palette but app theme and ANSI terminal theme remain separately configurable.
- Optional minimal mode: hideable workspace sidebar, hideable tab bar where a single tab, compact pane headers, no decorative animations.
- Theme must never follow OS dark mode unless a user explicitly selects `system` (default is fixed light).
- Settings: terminal font, shell, scrollback, workspace root, theme, shortcut override, performance options. Store settings in versioned TOML or JSON.
- Minimum keyboard accessibility: visible focus, sensible tab order, labels/tooltips, sufficient light-theme contrast.

### FR-07 Data durability and lifecycle — P0

- Atomic metadata writes or SQLite transactions; schema migrations; reject version downgrade that could corrupt data.
- Persist UI metadata debounced (e.g. 250–500ms), with explicit flush on clean exit; do not synchronously write on each keypress or output chunk.
- A single instance lock protects active user profile data (allow an explicit different profile directory for testing).
- Distinguish `close pane`, `stop process`, `hide pane`, `quit app`; document what happens to OS subprocesses on quit/crash.
- Crash recovery is limited to saved metadata and last known state; running sessions must be shown as disconnected/terminated until recreated.

## 6. Architecture and boundaries

```text
Desktop executable (Rust)
  GPUI presentation
    Application chrome / tab bar / sidebar / palette
    Layout renderer -> PanelView<Terminal> and PanelView<Agent>
    GPU terminal-cell renderer
         | typed commands + model subscriptions
  Application services
    Command registry / WorkspaceService / LayoutService
    TerminalSessionService / AgentService / ConfigService
         | explicit async boundaries
  Core domain (no GPUI dependencies)
    IDs + Workspace + Tab + LayoutNode + Panel + Agent
    Actions / reducers / validation / events
  Infrastructure adapters
    PTY (portable-pty) / VT model (alacritty_terminal)
    Process supervisor / SQLite / Config / OS integration
         |
  Host operating system
    Shells, child processes, filesystem, clipboard
```

**Do not couple GPUI views to OS process handles.** UI holds immutable snapshots / stable IDs and dispatches typed actions. Services own mutable handles, channels and lifecycles. Terminal data path: PTY read -> bounded channel -> VT state update -> dirty region/event -> GPUI draw; keyboard -> terminal input encoder -> PTY write. Backpressure must not deadlock process output.

**Ownership rules:** Workspace owns Tab IDs; Tab owns layout tree; layout leaf owns Panel ID; TerminalSession exists independently of Pane and is referenced by stable ID; Agent entity refers to TerminalSession ID and external child process metadata. Deleting visual layout must invoke an explicit session disposition action rather than implicitly kill a process.

**Rendering:** Keep offscreen terminal VT state current. Throttle frame production on hidden tabs. Never stop draining PTY output merely to save CPU. Benchmark and choose batching limits based on measured log output.

## 7. Domain model and identifiers

Use UUIDv7 or ULID for stable identities and store IDs as strings in SQLite. Runtime-only process handles must not be serialized.

```rust
struct Workspace { id: WorkspaceId, name: String, root: Option<PathBuf>,
  tab_order: Vec<TabId>, active_tab: Option<TabId>, archived: bool }
struct Tab { id: TabId, workspace_id: WorkspaceId, title: Option<String>,
  layout: LayoutNode, focused_panel: Option<PanelId> }
enum LayoutNode { Leaf { panel_id: PanelId },
  Split { axis: Axis, ratio: f32, first: Box<LayoutNode>,
          second: Box<LayoutNode> } }
enum Axis { Horizontal, Vertical }
struct Panel { id: PanelId, kind: PanelKind, title: String,
  resource: ResourceRef }
enum PanelKind { Terminal, Agent /* future registered kinds */ }
enum ResourceRef { Terminal(TerminalSessionId), Agent(AgentId) }
struct TerminalSession { id: TerminalSessionId, cwd: PathBuf,
  shell: PathBuf, process_status: ProcessStatus /* runtime state */ }
struct Agent { id: AgentId, provider: String, workspace: WorkspaceId,
  terminal_session: TerminalSessionId, cwd: PathBuf,
  state: AgentState, started_at: Option<DateTime>, ended_at: Option<DateTime> }
```

In code, avoid ambiguous nested ownership between Panel/Terminal/Agent. A managed agent is displayed via a terminal panel with agent metadata attached; decide one canonical representation and test it. The above types are conceptual API shapes, not drop-in compiled source.

**Schema tables (initial):** `schema_migrations`, `workspaces`, `tabs`, `panels`, `layouts` (serialized validated tree), `agents`, `app_settings`. Keep `runtime_sessions` in memory. `agent_runs` history is optional for Phase 1; do not store raw transcripts or credentials by default.

## 8. Interfaces and events

```text
WorkspaceService: create/rename/switch/archive/list/restore
TabService: create/close/reorder/focus
LayoutService: split/resize/move/maximize/close/focusNeighbor/validate
TerminalService: spawn/write/resize/subscribeOutput/terminate/getSnapshot
AgentService: launch/list/focus/stop/subscribeStatus
CommandRegistry: register/search/execute/listBindings
SettingsService: get/set/subscribe/import/export
```

**Domain events:** `WorkspaceChanged`, `TabActivated`, `LayoutUpdated`, `PanelFocused`, `TerminalCreated`, `TerminalExited`, `AgentStatusChanged`, `SettingsUpdated`.

**Threading rule:** the UI thread never waits synchronously for a PTY, filesystem scan, database migration, subprocess exit or I/O flush. Events should include stable IDs and monotonically increasing versions where ordering matters. Keep bounded buffers and counters for dropped/coalesced *UI render notifications*; never silently drop raw PTY bytes.

## 9. Agent CLI integration design

Each adapter provides: `id`, `display_name`, executable discovery, `build_command(profile, cwd) -> executable + argv + env whitelist`, optional documented `resume_command`, and process exit classification. Generic adapter works for arbitrary user-supplied CLI commands.

**Spawn flow:** user chooses workspace + agent profile -> validate allowed cwd/executable -> create runtime TerminalSession -> attach Agent metadata -> start PTY child -> update state from OS process and exit events -> stream output to terminal view.

**Stopping:** first send a graceful interrupt signal/input where possible; if timeout passes, show explicit option to force-terminate process group. Avoid killing unrelated processes; ensure child process ownership is recorded. Do not claim session resume unless the installed agent CLI actually provides a compatible resume capability and this has been tested.

**Security:** inherit only intended environment; never log environment variables, prompt contents, tokens, SSH private keys or stdout by default. Command execution should use argv vectors and OS APIs, not shell-expanded strings. Profiles are local data. Do not invoke agents automatically when merely restoring workspace layout.

## 10. Keyboard and interaction specification

- Opening app restores last active workspace and shows a focused terminal.
- `New workspace` prompts for name and optional root, creates default tab with one shell.
- `Split` retains current PTY and spawns *one* new PTY at the new panel location; current tab remains active.
- `Move panel` preserves existing terminal session ID and current output/state.
- `Close tab` with live processes shows a modal with Cancel / Close and terminate; do not use a misleading 'detach' option without a supporting daemon.
- `Agent launch` lets user choose provider, cwd and profile, then creates a visible terminal pane and agent listing.
- `Workspace switch` is instantaneous from the user's perspective and does not spawn/kill sessions.
- `Quit` warns that running local sessions/agents will end; provide a setting to avoid warning only if user opts in.

## 11. Visual specification

Window: light native chrome, low-contrast borders, compact top tab strip, collapsible left workspace rail, full-height pane grid. Panels have subtle headers with process title, cwd short label, running indicator, split/maximize/close affordances. A small global palette can open any workspace, tab, command or agent. Agent Center can be a dockable built-in panel or sidebar overlay; it must not force permanent dashboard chrome over terminals.

**Design tokens:** semantic names `surface.canvas`, `surface.subtle`, `text.default`, `text.muted`, `border.default`, `accent.primary`, `status.running`, `status.failed`, not hard-coded hex values in individual components. GitHub Light values are starting values, not an official GitHub theme distribution. Use a single `Theme` struct and persist choice per profile; optional workspace overrides are Phase 1 stretch.

## 12. Performance and resource budgets

Targets are **nonbinding initial budgets until measured on agreed hardware**, not existing benchmark claims. Measure app process tree (GUI plus its owned subprocesses) and separately report the GUI's own memory footprint.

| Metric | Initial target / test |
|---|---|
| Cold start to interactive terminal | p50 < 700ms on Apple Silicon, measured clean launch |
| UI base memory, one idle terminal | target < 120 MiB app-only resident footprint |
| App memory, 10 idle terminals | target < 300 MiB app-only (exclude shell children, report separately) |
| Idle CPU after stabilization | < 1% of one core on test hardware |
| Workspace/tab switch | p95 < 75ms visual response |
| Resize/render behavior | no sustained frame stalls > 100ms during normal interaction |
| Stress case | 20 terminals, 3 workspaces, 5 agents, scripted output, no hangs/leaks |

**Benchmark methodology:** log commit, hardware, OS, build mode, process boundaries, terminal dimensions, scrollback limits, shell type, workload, warm/cold state, 5 runs with median/p95. Compare to an unmodified Ghostty and optionally Wave under the *same workload* for context; don't claim parity from subjective feel. Profile RSS, allocations, CPU samples, frame timings and terminal input-to-paint latency where instrumentable.

## 13. Reliability and security

- Protect PTYs from UI thread stalls; cap pending render state, not stdout bytes.
- Handle process exit, child crash, rapid tab close, resize storms, massive output, Unicode edge cases and invalid/corrupt layouts.
- Never deserialize a path into executable arguments without validation. No auto-execution on workspace import or session restore.
- File operations limited to workspace/config metadata in P0; avoid creating a privileged background service.
- No plugin runtime in Phase 1; future plugins require explicit capability grants, sandboxing, isolated failures and versioning.
- Agent status is observed from managed subprocess handles and documented integrations only. Never guess 'waiting for approval' from generic terminal text.
- Local SQLite backup / migration rollback strategy; fail gracefully with recovery message if data store cannot open.

## 14. Repository blueprint

```text
workspace/
  Cargo.toml                       # workspace dependency pinning
  rust-toolchain.toml
  crates/
    app/                           # binary + composition root
    ui/                            # GPUI views, theme, keybindings
    domain/                        # IDs, models, invariants, actions
    workspace-service/             # workspaces/tabs/layout coordination
    terminal-core/                 # VT state, PTY abstraction, I/O
    agent-core/                    # profiles, supervised agents
    persistence/                   # SQLite, migrations, settings
    platform/                      # OS clipboard, filesystem, launch
    observability/                 # tracing, benchmarks, profiling
  assets/themes/github-light.toml
  docs/adr/
  docs/architecture/
  tests/integration/
  benchmarks/
  scripts/
```

**Dependency direction:** `domain` imports no UI/framework/storage crate. `ui` depends on service interfaces; services depend on domain and abstract infrastructure traits; concrete infrastructure wired in `app`. Avoid circular crates, generic plugin abstractions beyond the panel type contract, and premature event buses for trivial in-process calls.

## 15. Build plan and agent work allocation

**Gate 0: Architectural proof (must precede parallel feature coding)**
- Spike A: GPUI builds and launches on macOS; verify Linux smoke feasibility and text/IME support.
- Spike B: terminal PTY -> alacritty parser -> GPUI renderer; test vi, htop, ssh, Unicode, truecolor, resize, clipboard.
- Spike C: one child process clean startup/stop; capture exit; confirm cancellation behavior.
- Spike D: measure clean-app baseline memory and startup; capture profile artifacts.
- Deliver ADR-001 confirming stack or proposing alternatives with evidence. Dependency versions are pinned after this.

**Workstream A — Domain & persistence**
Own stable IDs, layout tree operations, invariant tests, workspace/tab reducers, SQLite schema/migrations, restoration policy. Deliver type-safe APIs plus deterministic tests; no GPUI imports.

**Workstream B — Terminal**
Own PTY process interface, VT state, input encoding, terminal renderer, scrollback, resizing, clipboard/selection and terminal fidelity test suite. Integrate through stable events only.

**Workstream C — UI/UX**
Own GPUI app shell, GitHub Light tokens, workspace rail, tab bar, pane chrome, splits/resizing, focus navigation, command palette, customizable keymaps.

**Workstream D — Agents**
Own adapter registry, executable resolution, CLI profiles, agent lifecycle, agent center, safe stop behavior, agent data migration/tests; consume existing terminal session interface.

**Workstream E — Integration/QA**
Own CI, packaging, crash/recovery scenarios, stress harness, profiling scripts, documentation, Linux smoke coverage. No large feature implementation until integration contracts land.

### Parallelization and dependencies

1. **Serial foundation:** Gate 0, contracts, ID schemas, first working terminal loop.
2. **Parallel after contracts:** Domain/persistence and UI chrome; terminal fidelity and agent adapter skeleton.
3. **Integration:** tabs/splits/workspaces + terminal lifecycle; then managed agent processes.
4. **Hardening:** persistence recovery, keyboard/accessibility, Linux validation, performance and packaging.

Agents must coordinate through typed interfaces and ADRs; do not independently refactor common primitives. Every workstream opens small PRs with demo, tests and performance notes where relevant.

## 16. User stories and test cases

**US-01 — Start shell:** As user, I open app and have an interactive shell with correct cwd, colors and Ctrl-C. **Pass:** `echo`, `git status`, `vim`, `htop` and shell resize work.

**US-02 — Multiplex:** I create 3 tabs, split one into 4 panes, resize and move focus entirely with shortcuts. **Pass:** all terminals preserve their processes/output during operations.

**US-03 — Workspace isolation:** I create `Personal`, `Infrastructure`, `Project` workspaces. **Pass:** each retains its own tabs/layout and switching never restarts shells.

**US-04 — Launch agent:** I start Claude CLI in a selected repository and Codex CLI in another. **Pass:** both are separate managed sessions with accurate OS-derived run/exit states and focus navigation.

**US-05 — Close safely:** I try closing a pane running an agent. **Pass:** confirmation prevents accidental child termination; cancel preserves running session.

**US-06 — Restart:** I quit and reopen. **Pass:** metadata/layout restored, *new shells* instantiated, historical agents marked exited/disconnected; no fake resumption and no auto-start of agent CLI.

**US-07 — Settings:** I set light theme, hide sidebar and change shortcut. **Pass:** survives restart and settings migration.

**US-08 — Failure:** Simulate invalid cwd, nonexistent agent CLI, PTY spawn failure, corrupt layout and agent exit. **Pass:** focused recoverable errors, no application crash or lingering unintended child processes.

**US-09 — Stress:** Run 20 panes with generated output and switch between three workspaces for 15 minutes. **Pass:** no deadlock, measured resource budgets tracked; only benchmark regressions with approved justification allowed.

**US-10 — Linux:** compile and launch, create shell, split, clipboard and restart metadata on one supported Linux distribution/display server. **Pass:** documented supported target and known limitations.

## 17. QA strategy

- **Unit:** layout transformations, IDs, state reducer invariants, keybinding conflict checks, agent state machine, config validation.
- **Integration:** PTY lifecycle, resize signal, exit propagation, UI dispatch and persistence transaction failure.
- **Terminal compatibility:** interactive ncurses utilities, ANSI sequences, bracketed paste, Unicode widths, alternate screen, selection/copy, high-throughput output.
- **Manual exploratory:** macOS multi-display, Retina scaling, rapid focus moves, terminal apps using mouse, low-power mode; Linux under documented display stack.
- **Performance:** automated repeatable workload with thresholds reported in CI or release checklist, not brittle uncalibrated universal pass/fail.
- **Security:** environment redaction in logs, safe argument execution, correct process-group teardown, no secret persistence.

## 18. Delivery checklist and definition of done

MVP can be called done only when all P0 user stories US-01 through US-08 pass on supported macOS hardware; no known data-loss or orphan-process defect; baseline benchmarks recorded; user can install/use the app without developer tooling; app is uniquely branded and not a Wave fork.

Phase 1 is done when US-01 through US-10 pass, keymaps are configurable, UI is consistently light, packaging and migrations are tested, Linux smoke is documented, and performance budget deviations have an explicit issue/ADR and measured evidence.

**Required handoff artifacts:** app binaries/packaging scripts, architecture diagram, crate/API docs, ADRs, SQLite migration scripts, theme tokens, keymap reference, benchmark results, test reports, known limitations, and changelog.

## 19. Future phases: strict extension seams

**Phase 2:** service daemon for persistent local sessions and reattachment; SSH host manager; Git worktree-aware agents; richer agent status via official hooks; optional remote access. Requires threat model, reconnect protocol and explicit crash/restart semantics.

**Phase 3:** browser panel using platform webviews (macOS WebKit, Linux WebKitGTK, Windows WebView2 or alternate evaluated engine); editor panel with LSP, file watching; database/cloud panels through plugins. Webview embedding inside GPUI is a research risk and must be validated independently before promising browser features.

**Phase 4:** public plugin SDK, capability permissions, isolated plugin processes or sandbox, signing/trust, versioned APIs, marketplace optional. Panel abstraction stays stable but **no arbitrary plugin code** executes in P0.

## 20. Risks, decisions and stop rules

| Risk | Impact | Mitigation / go-no-go |
|---|---|---|
| GPUI API or Linux platform friction | High | Gate 0 builds and smoke tests; ADR for alternative UI if unacceptable |
| Terminal fidelity/rendering complexity | High | Reuse alacritty_terminal; automated VT tests and known app suite |
| Agent status ambiguity | Medium | OS lifecycle only; provider hooks later; show confidence/unknown |
| Resource use with many sessions | High | Bounded scrollback, lazy views, background frame throttling, profiling |
| Killing wrong child/process group | High | Explicit supervisor ownership, defensive confirmation, lifecycle tests |
| Future browser in GPUI | High | Keep panel contract generic; prove native webview composition in later spike |
| Overbuilding plugins in v1 | High | No plugin runtime until actual second panel implementation needs it |

**Stop conditions:** Do not proceed with broad application feature work until Gate 0 terminal rendering is usable, and the PTY lifecycle is safe. Do not claim session persistence across app restarts until an independent daemon exists. Do not optimize blindly: measure before replacing libraries.

## 21. Instructions to engineering agents (copy into agent task context)

You are implementing a NEW native Rust project described in NDW-PRD-001. Do not fork Wave or create an Electron app. Do not introduce browsers, editors, databases, cloud integrations or plugin runtimes in Phase 1. Begin with Gate 0 feasibility and ADR-001, then submit a repository skeleton and a working interactive terminal. Use GPUI for desktop UI, alacritty_terminal for VT state and portable-pty for PTY unless feasibility findings justify an ADR. Keep domain independent of GPUI. Build features behind stable service interfaces, include automated tests for state transformations and lifecycle, and benchmark RAM/CPU/startup with every important architectural change. Never equate layout restoration with process persistence. Never log secrets or auto-run agent commands when restoring layouts. Do not claim completion until acceptance criteria and benchmarks are demonstrated on real hardware. Surface blockers with evidence and the smallest viable alternative.

### Suggested implementation sequence / tickets

- **ARCH-001**: GPUI + terminal library compatibility spike, benchmark harness, ADR.
- **CORE-001**: Cargo workspace, tracing, error taxonomy, config schema, domain IDs.
- **TERM-001**: PTY spawn + read/write + resize + clean termination.
- **TERM-002**: VT grid render and accurate keyboard/clipboard support.
- **LAYOUT-001**: Binary split tree and invariant tests.
- **UI-001**: Window/chrome/GitHub Light theme and focused terminal.
- **UI-002**: Tabs, panes, resize, maximize and key navigation.
- **WS-001**: Named workspaces and state reducers.
- **DATA-001**: SQLite migrations, restore metadata, corruption fallback.
- **CMD-001**: Command palette and default shortcut registry.
- **AGENT-001**: Managed CLI adapter profiles and safe subprocess lifecycle.
- **AGENT-002**: Agent center and focus/status/stop actions.
- **QA-001**: Integration suite, stress harness, input/output fidelity.
- **REL-001**: macOS packaging, install validation, performance report.
- **REL-002**: Linux smoke and documented support matrix.

## 22. Reference implementations and documentation

- GPUI: https://github.com/zed-industries/zed/tree/main/crates/gpui
- Zed terminal architecture (reference only): https://github.com/zed-industries/zed/tree/main/crates/terminal
- Alacritty terminal crate: https://docs.rs/alacritty_terminal/
- portable-pty: https://docs.rs/portable-pty/
- SQLite: https://www.sqlite.org/docs.html
- Rust tracing: https://docs.rs/tracing/

**License review:** Before copying any code, verify applicable licenses and dependencies. This document directs reuse of library APIs, not copying entire applications.
