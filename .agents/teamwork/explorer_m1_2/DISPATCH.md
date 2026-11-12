# Dispatch: Explorer 2 (M1 SMTC Cover Change Detection)

## Identity
- Archetype: teamwork_explorer
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\explorer_m1_2

## Task
Investigate Feature 2 (F2): Late SMTC Cover Byte Detection in `src/monitor.rs` and `src/ui/mod.rs`.
Do NOT write code or modify source files.

## Mandatory Reading
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md`
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- `e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md`
- `e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md`

## Specific Questions to Answer
1. In `src/monitor.rs`, how are media thumbnails fetched via `extract_smtc_thumbnail`, and why do cover bytes often arrive on subsequent ticks after metadata?
2. In `src/ui/mod.rs`, inspect how `cur_cover` and `prev_key` are constructed and compared in the polling loop. Why does the current comparison fail when `cover_bytes` arrives late?
3. What is the exact proposed key formula incorporating `n.play_id`, `n.cover`, and `n.cover_bytes` length?
4. How should image decoding be handled (`decode_bytes_to_render_image` vs `fetch_cover_render_image`)?
5. How can we write a dedicated unit test in `src/monitor.rs` or `tests/` to verify that `cover_bytes` arrival triggers key divergence?

Write your analysis and recommendations to `e:\杂项\music-hub\.agents\teamwork\explorer_m1_2\handoff.md`.


## 2026-11-12T03:08:43Z
From: 0938fee6-a149-4028-954d-d64c1f44f0ec
Content:
You are Explorer 2 for Milestone 1.
Your working directory is: e:\杂项\music-hub
Your state folder is: e:\杂项\music-hub\.agents\teamwork\explorer_m1_2
Read your instructions in: e:\杂项\music-hub\.agents\teamwork\explorer_m1_2\DISPATCH.md
Mandatory reading:
- e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md
- e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md
- e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md
- e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md

Investigate Feature 2 (F2): Late SMTC Cover Byte Detection in src/monitor.rs and src/ui/mod.rs.
Answer the specific questions in DISPATCH.md.
Do NOT modify source files or execute build/write tasks.
Produce a thorough investigation report and write it to e:\杂项\music-hub\.agents\teamwork\explorer_m1_2\handoff.md.
When done, send a message to your parent reporting completion.


## 2026-11-12T03:21:14Z
From: 0938fee6-a149-4028-954d-d64c1f44f0ec
Content:
**Context**: Status check on Explorer 2 investigation
**Content**: Please report your current progress on F2 investigation. If you have completed your analysis, please write handoff.md and report back.
**Action**: Update progress.md, finish handoff.md, and reply with completion status.
