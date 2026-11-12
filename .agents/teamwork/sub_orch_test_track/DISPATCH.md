# Dispatch: E2E Testing Orchestrator

## Identity & Role
- Archetype: self (sub-orchestrator)
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\sub_orch_test_track
- Parent: orchestrator (ea25af0d-d74a-45d4-bd84-e58825762a14)

## Objective
Lead the E2E Testing Track for Music Hub. Design and implement a comprehensive opaque-box and automated test suite derived from `ORIGINAL_REQUEST.md` and user-facing requirements (R1–R4).

## Reference Documents
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md`
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- Survey reports:
  - `e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md`
  - `e:\杂项\music-hub\.agents\teamwork\explorer_survey_2\survey_r2_r3.md`
  - `e:\杂项\music-hub\.agents\teamwork\explorer_survey_3\survey_r4.md`

## Testing Requirements & Methodology
Follow the 4-tier test case design methodology from Project Pattern:
- **Tier 1: Feature Coverage (>=5 per feature)**
- **Tier 2: Boundary & Corner Cases (>=5 per feature)**
- **Tier 3: Cross-Feature Interactions**
- **Tier 4: Real-World Scenarios**

Exclusive Write Ownership:
- `tests/` directory and test infrastructure files.

## Deliverables
1. `TEST_INFRA.md` at project root (`e:\杂项\music-hub\TEST_INFRA.md`) detailing the test suite architecture and feature inventory.
2. Automated tests in `tests/` that compile with `cargo test`.
3. When test suite creation and initial verification are complete, publish `TEST_READY.md` at project root (`e:\杂项\music-hub\TEST_READY.md`).
4. `handoff.md` in your state folder.


## 2026-11-12T03:02:52Z
[Message] sender=ea25af0d-d74a-45d4-bd84-e58825762a14
You are the E2E Testing Orchestrator for music-hub.
Your working directory is e:\杂项\music-hub.
Your state folder is e:\杂项\music-hub\.agents\teamwork\sub_orch_test_track.
Your dispatch instructions are in e:\杂项\music-hub\.agents\teamwork\sub_orch_test_track\DISPATCH.md.
Read e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md and e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md.
Follow the E2E Testing Track instructions in Project Pattern:
1. Assess & decompose requirements into test tiers.
2. Produce e:\杂项\music-hub\TEST_INFRA.md.
3. Write automated tests in tests/ covering Tiers 1-4.
4. Verify with cargo test.
5. Publish e:\杂项\music-hub\TEST_READY.md when complete.
6. Write handoff.md in your state folder and send a message to parent when done.
