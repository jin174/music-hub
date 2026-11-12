# Dispatch: Explorer 1 (M1 Lifecycle & Window Management)

## Identity
- Archetype: teamwork_explorer
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\explorer_m1_1

## Task
Investigate Feature 1 (F1): Long-lived GPUI Application and Window Hide/Restore Lifecycle in `src/ui/mod.rs`.
Do NOT write code or modify source files.

## Mandatory Reading
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md`
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- `e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md`
- `e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md`

## Specific Questions to Answer
1. In `src/ui/mod.rs:run_console_loop`, how should `Application::new().run(...)` be restructured to run as a single long-lived instance rather than an outer loop?
2. How should `window.on_window_should_close` be registered to intercept `WM_CLOSE`, call Win32 `ShowWindow(hwnd, SW_HIDE)`, and return `false`?
3. How should `get_console_hwnd()` locate the window handle reliably by title and process ID?
4. In the background command receiver (`ConsoleCommand::Open`), how should the window be restored via `ShowWindow(hwnd, SW_RESTORE)`, `SetForegroundWindow`, and `window.activate_window()`?
5. What edge cases exist (e.g. initial launch with `open_on_start = false` vs `true`, tab switching, toast display)?

Write your analysis and recommendations to `e:\杂项\music-hub\.agents\teamwork\explorer_m1_1\handoff.md`.


## 2026-11-12T03:08:42Z
You are Explorer 1 for Milestone 1.
Your working directory is: e:\杂项\music-hub
Your state folder is: e:\杂项\music-hub\.agents\teamwork\explorer_m1_1
Read your instructions in: e:\杂项\music-hub\.agents\teamwork\explorer_m1_1\DISPATCH.md
Mandatory reading:
- e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md
- e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md
- e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md
- e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md

Investigate Feature 1 (F1): Long-lived GPUI Application and Window Hide/Restore Lifecycle in src/ui/mod.rs and src/main.rs.
Answer the specific questions in DISPATCH.md.
Do NOT modify source files or execute build/write tasks.
Produce a thorough investigation report and write it to e:\杂项\music-hub\.agents\teamwork\explorer_m1_1\handoff.md.
When done, send a message to your parent reporting completion.


## 2026-11-12T03:21:00Z
**Context**: Status check on Explorer 1 investigation
**Content**: Please report your current progress on F1 investigation. If you have completed your analysis, please write handoff.md and report back.
**Action**: Update progress.md, finish handoff.md, and reply with completion status.
