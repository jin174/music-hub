# Dispatch: Test Track Explorer 1 (Infra & R1-R2 Mapping)

## Identity & Role
- Archetype: teamwork_explorer
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\test_track_explorer_1
- Parent: E2E Testing Orchestrator (sub_orch_test_track)

## Task Description
Read:
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md` (MANDATORY)
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- `e:\杂项\music-hub\.agents\teamwork\sub_orch_test_track\SCOPE.md`
- `e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md`
- `e:\杂项\music-hub\.agents\teamwork\explorer_survey_2\survey_r2_r3.md`

Investigate:
1. Test suite architecture in `tests/`: How to structure test files in `tests/` for `MusicHub` (a binary crate) so that `cargo test` compiles and runs them reliably in headless Windows CI environments.
2. Concrete test case specifications for Requirements R1 & R2 (Features F1–F6):
   - F1: Long-lived GPUI Application (hide/show lifecycle, command channels, state persistence)
   - F2: Late SMTC Cover Byte Detection (cache key logic, byte arrival transitions)
   - F3: Repaint Gating & Decoupled Toasts (state comparison logic, toast timer decoupling)
   - F4: Scrobble Error Visibility (`last_error` propagation in `AppStateResponse` and UI models)
   - F5: Bottom Bar Heart Synchronization (feedback toggle in SQLite, radar/roam/blacklist sync)
   - F6: OAuth Settings Synchronization (config update to UI inputs sync)
3. For each feature F1–F6, specify >=5 Tier 1 tests and >=5 Tier 2 tests, plus relevant Tier 3 combinations.

## Deliverable
Write your full findings and recommendations to:
`e:\杂项\music-hub\.agents\teamwork\test_track_explorer_1\handoff.md`
## 2026-11-12T03:14:10Z
[Message] timestamp=2026-11-12T03:14:10Z sender=f119d8d4-4796-4e92-aede-5db30c51d61c priority=MESSAGE_PRIORITY_HIGH content=You are Test Track Explorer 1 for music-hub.
Your working directory is e:\杂项\music-hub.
Your state folder is e:\杂项\music-hub\.agents\teamwork\test_track_explorer_1.
Your task instructions are in e:\杂项\music-hub\.agents\teamwork\test_track_explorer_1\DISPATCH.md.
MANDATORY: Read e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md before starting work.
Also read e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md and e:\杂项\music-hub\.agents\teamwork\sub_orch_test_track\SCOPE.md.
Perform technical investigation on test suite architecture in tests/ and detailed test specifications for R1 & R2 (F1–F6) covering Tiers 1-3.
Write your structured findings to e:\杂项\music-hub\.agents\teamwork\test_track_explorer_1\handoff.md.
When finished, send a message to the caller (Recipient: "f119d8d4-4796-4e92-aede-5db30c51d61c") reporting completion and referencing the handoff path.

