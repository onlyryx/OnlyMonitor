# System Awareness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add focused process resource attribution, live tray summaries and opt-in local high-load notifications without turning Only Monitor into a process manager.

**Architecture:** Keep the existing Rust sampler as the only system sampling loop. Return bounded process rankings beside, but separate from, the `Snapshot` history; use the same sampled state to update the tray tooltip and a pure high-load detector; expose the new state through the existing typed `get_monitor_state` command. React adds a resource-source page and one opt-in notification setting.

**Tech Stack:** Existing Tauri 2, React, TypeScript, Vite, sysinfo 0.37.2, ts-rs, Vitest; official Tauri notification plugin for installed Windows builds.

**Spec:** `docs/superpowers/specs/2026-10-27-system-awareness-design.md`

## Global Constraints

- One system sampling worker; process data is not copied into the 60-second system history.
- Process rows are local, current-session only, capped at 8 per ranking and never persisted or uploaded.
- No process control, command-line or executable-path collection, administrator requirement, arbitrary shell, AI, or external monitoring requests.
- Alert setting defaults off; CPU or memory must be at least 90% for 30 seconds; one notification per high-load episode; re-arm after staying below 80% for 60 seconds.
- Update the app/package version to 0.5.0 only after the feature and its release checks are ready.

## Review Focus

- New process or reused PID: never inherit another process's CPU baseline.
- Suspend/resume, clock jump, failed sample: do not present interval-spanning CPU usage or count failures toward an alert.
- Large process lists: return only top eight per ranking and never grow the retained history with process rows.
- Alert boundary crossings: avoid notifications at 90% for less than 30 seconds and repeated notifications before 60 seconds below 80%.
- Protected/unavailable processes and notification delivery failure: keep ordinary system sampling alive and report the limitation without inventing zero values.

---

### Task 1: Add bounded process resource snapshots

**Files:** `src-tauri/src/model.rs`, `src-tauri/src/collector.rs`, `src-tauri/src/runtime.rs`, `src/bindings.ts`, existing Rust tests.

**Interfaces:**
- Produces `ProcessInfo { pid, name, cpu_usage_percent, resident_memory_bytes }`, `ProcessSnapshot { sampled_at_ms, top_cpu, top_memory }` and `MonitorState.process_snapshot` / `process_error`.
- `ProcessSnapshot` is current-only and is not a field of `Snapshot` or `HistoryEntry`.
- Collector returns an internal sample containing the existing system `Snapshot` and its process snapshot; runtime stores the two on the same sample attempt.

- [x] Write Rust tests for separate CPU/memory top-8 rankings, CPU normalization, newly observed process baseline, PID reuse, and an error history entry containing no process list.
- [x] Run those focused tests and confirm the new behavior is missing.
- [x] Add the DTO and generate `src/bindings.ts` with the existing `UPDATE_BINDINGS=1` workflow.
- [x] Refresh only process CPU and resident memory from the existing `System`; identify rows by PID plus start time, reuse collector interruption logic, normalize CPU by logical CPU count, and return the top eight per ranking.
- [x] Update runtime to replace current process state on each sample and leave bounded historical `Snapshot` entries process-free.
- [x] Run focused Rust tests, `frontend_contract_matches_rust`, and `npm run typecheck`.

### Task 2: Add opt-in high-load policy and schema migration

**Files:** `src-tauri/src/alert.rs` (new), `src-tauri/src/settings.rs`, `src-tauri/src/model.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/alert.rs` tests.

**Interfaces:** `HighLoadAlert::observe(enabled, cpu, memory, now: Instant) -> Vec<LoadMetric>` returns newly triggered metrics only. Settings schema 3 adds `high_load_alert_enabled` (serialized as `highLoadAlertEnabled`), default false.

- [x] Write unit tests for 90%/30-second trigger, below-80%/60-second re-arm, one notice per episode, independent CPU/RAM episodes, and error/missing samples resetting continuous-time measurement.
- [x] Run the new tests and confirm they fail for the missing detector.
- [x] Implement the pure detector with monotonic `Instant` state and fixed thresholds; gaps over 10 seconds restart continuous timing.
- [x] Write settings tests for schema 2 migration to schema 3 with alerts off, schema 1 chained migration, preservation of old values, and no disk rewrite during load.
- [x] Implement schema 3 validation, default and migration.
- [x] Run focused Rust tests and the full settings test module.

### Task 3: Integrate live tray summary and native notifications

**Files:** `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/src/lib.rs`, `src-tauri/src/desktop.rs`, `src-tauri/src/runtime.rs`, notification/tray formatting tests.

**Interfaces:** One post-sample observer receives the resulting `MonitorState`; it updates the existing tray tooltip and feeds the alert detector. Notification errors are contained and recorded separately from sampling errors.

- [x] Add tests for concise tray text on initializing, valid, and failed states; detector trigger policy and notification opt-in are covered by Rust and settings tests.
- [x] Confirm the new behavior through the focused Rust and frontend test runs.
- [x] Add a compatible pinned Tauri notification plugin dependency, register the plugin and use its Rust desktop API; no frontend notification API or permission was needed.
- [x] Pass a single post-sample observer into the existing worker; update the tray tooltip from the latest state and send a Windows toast only for newly triggered metrics while the saved preference is enabled.
- [x] Ensure notification or tooltip failure cannot stop the collector; report notification errors independently.
- [x] Run focused Rust tests, `cargo fmt --check`, and `cargo clippy --all-targets -- -D warnings`.

### Task 4: Build the focused Activity page

**Files:** `src/App.tsx`, `src/styles.css`, `src/App.test.tsx`, `src/testFixtures.ts`.

- [x] Write UI tests for Overview-to-Activity navigation, dual CPU/memory rankings, top-eight rendering, warming CPU baseline, no-data state, and no process-management controls.
- [x] Run the new UI tests and confirm the UI behavior.
- [x] Add the `Activity` route, bounded process rows and Overview “查看负载来源” entry using the existing chart/card/navigation patterns.
- [x] Display CPU normalized as whole-system capacity, resident memory wording, sample time, protected-process limitation, and both resource values per process row.
- [x] Run the focused UI test, then all frontend tests and typecheck.

### Task 5: Add notification preference and user-facing documentation

**Files:** `src/SettingsPanel.tsx`, `src/settings.test.tsx`, `README.md`, `CHANGELOG.md`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, package version manifests as needed.

- [x] Add Settings tests for default-off behavior, explicit enable, save failure and restored saved state.
- [x] Run the Settings tests.
- [x] Add the single opt-in toggle and concise explanation of 90%/30-second trigger, one alert per metric episode and 80%/60-second re-arm; preserve draft/save behavior.
- [x] Update README and changelog with the three capabilities, local-only process data, process-data limitations, notification install requirement and non-manager scope.
- [x] After feature tests were green, align application/package/Cargo versions to 0.5.0 and verify lockfiles/build metadata agree.
- [x] Run full frontend tests, typecheck and Rust tests.

### Task 6: Release and installed-app verification

**Files:** `docs/v0.5-verification.md` (new), generated release artifacts.

- [x] Run `npm test`, `npm run typecheck`, `npm run build`, `cargo fmt --check`, `cargo test --manifest-path src-tauri/Cargo.toml`, and Clippy.
- [x] Build the Tauri NSIS Release. Do not install over the currently running v0.4 application during this implementation turn.
- [ ] Install the generated package and verify true process names and changing CPU/RAM rankings; enable notifications explicitly; use a controlled workload to check threshold timing, one-per-episode behavior and recovery; inspect the tray tooltip while visible and hidden.
- [ ] Verify system metrics/history remain correct, process rows do not enter history, and no process-control UI or network request is introduced.
- [x] Record passed and unverified items separately; do not mark toast behavior passed based only on development mode.

## Repository Note

The workspace currently has no Git repository. Do not create commits or claim a Git-based rollback. Preserve changes in place and retain the previous 0.4.0 Release artifacts as the rollback build.
