# Dispatch: Test Writer 1 (E2E Test Infrastructure & Test Suite Implementation)

## Identity & Role
- Archetype: teamwork_test_writer
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\test_track_worker_1
- Parent: E2E Testing Orchestrator (sub_orch_test_track, id: f119d8d4-4796-4e92-aede-5db30c51d61c)

## Mandatory Integrity Warning
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A teamwork_auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.

## Mandatory Reading
MANDATORY: Read before starting work:
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md` (authoritative user requirements)
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- `e:\杂项\music-hub\.agents\teamwork\sub_orch_test_track\SCOPE.md`
- `e:\杂项\music-hub\.agents\teamwork\test_track_explorer_1\handoff.md` (R1-R2 F1-F6 test specifications)
- `e:\杂项\music-hub\.agents\teamwork\test_track_explorer_2\handoff.md` (R3-R4 F7-F15 test specifications)
- `e:\杂项\music-hub\.agents\teamwork\test_track_spec_miner\handoff.md` (Tier 3-4 pairwise and scenario specifications + TEST_INFRA draft)

## Write Ownership
- `e:\杂项\music-hub\TEST_INFRA.md`
- `e:\杂项\music-hub\TEST_READY.md`
- `e:\杂项\music-hub\tests/` directory (all test files and fixtures)
- If required for integration tests in `tests/` to compile: `e:\杂项\music-hub\Cargo.toml` (adding `[lib]` entry) and `e:\杂项\music-hub\src\lib.rs`.

## Tasks & Deliverables
1. **Create `e:\杂项\music-hub\TEST_INFRA.md`**:
   Follow the Project Pattern template, documenting test philosophy (opaque-box, requirement-driven), feature inventory mapping F1–F15, test architecture, and coverage thresholds.
2. **Implement Automated Tests in `tests/` Covering Tiers 1–4**:
   - `tests/common/mod.rs`: Shared test fixtures, mock configurations, in-memory DB helpers, mock HTTP/SMTC helpers.
   - `tests/tier1_feature_tests.rs`: >=75 test cases covering happy path feature coverage for F1–F15 (>=5 per feature).
   - `tests/tier2_boundary_tests.rs`: >=75 test cases covering boundary values, empty inputs, edge cases, Unicode/emojis, 900x600 layout limits, and failure paths (>=5 per feature).
   - `tests/tier3_pairwise_tests.rs`: >=17 test cases covering pairwise cross-feature interactions.
   - `tests/tier4_scenario_tests.rs`: >=9 test cases covering real-world application scenarios.
3. **Verify with `cargo test`**:
   Run `cargo test` and verify that all test suites compile and execute cleanly with 0 failures.
4. **Publish `e:\杂项\music-hub\TEST_READY.md`**:
   Detail test runner command (`cargo test`), expected exit code 0, coverage summary table by tier, and complete feature checklist.
5. **Write `handoff.md`**:
   In your state folder `e:\杂项\music-hub\.agents\teamwork\test_track_worker_1\handoff.md` with: Observation, Logic Chain, Caveats, Conclusion, Verification Method.
   Send a completion message to the caller when done.
