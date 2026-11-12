# Dispatch: Explorer 1 Replacement (M1 Lifecycle & Window Management)

## Identity
- Archetype: teamwork_explorer
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep

## Task
Investigate Feature 1 (F1): Long-lived GPUI Application and Window Hide/Restore Lifecycle in `src/ui/mod.rs` and `src/main.rs`.
Do NOT write code or modify source files.

## Mandatory Reading
- `e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md`
- `e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md`
- `e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md`
- `e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md` (Specifically Section 3 & Section 6)

## Specific Questions to Answer
1. In `src/ui/mod.rs:run_console_loop`, how should `Application::new().run(...)` be restructured to run as a single long-lived instance rather than an outer loop?
2. How should `window.on_window_should_close` be registered to intercept `WM_CLOSE`, call Win32 `ShowWindow(hwnd, SW_HIDE)`, and return `false`?
3. How should `get_console_hwnd()` locate the window handle reliably by title and process ID?
4. In the background command receiver (`ConsoleCommand::Open`), how should the window be restored via `ShowWindow(hwnd, SW_RESTORE)`, `SetForegroundWindow`, and `window.activate_window()`?
5. What edge cases exist (e.g. initial launch with `open_on_start = false` vs `true`, tab switching, toast display)?

Write your analysis and recommendations to `e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep\handoff.md`.
When done, send a message to your parent reporting completion.


## 2026-11-12T03:29:36Z
[Message] timestamp=2026-11-12T03:29:36Z sender=0938fee6-a149-4028-954d-d64c1f44f0ec priority=MESSAGE_PRIORITY_HIGH
You are Explorer 1 (Replacement) for Milestone 1.
Your working directory is: e:\杂项\music-hub
Your state folder is: e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep
Read your instructions in: e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep\DISPATCH.md
Mandatory reading:
- e:\杂项\music-hub\.agents\teamwork\ORIGINAL_REQUEST.md
- e:\杂项\music-hub\.agents\teamwork\orchestrator_1\PROJECT.md
- e:\杂项\music-hub\.agents\teamwork\sub_orch_m1\SCOPE.md
- e:\杂项\music-hub\.agents\teamwork\explorer_survey_1\survey_r1.md (Pay special attention to Section 3 and Section 6 which already have deep code analysis).

Investigate Feature 1 (F1): Long-lived GPUI Application and Window Hide/Restore Lifecycle in src/ui/mod.rs and src/main.rs.
Answer the specific questions in DISPATCH.md.
Do NOT modify source files or execute build/write tasks.
Produce a thorough investigation report and write it to e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep\handoff.md.
When done, send a message to your parent reporting completion.


## 2026-11-12T03:36:28Z
[Message] timestamp=2026-11-12T03:36:28Z sender=0938fee6-a149-4028-954d-d64c1f44f0ec priority=MESSAGE_PRIORITY_HIGH
**Context**: Status check on Explorer 1 Replacement
**Content**: Please report your progress on F1 investigation. When ready, write your 5-component handoff report to handoff.md and notify me.
**Action**: Update progress.md and complete handoff.md.


## 2026-11-12T03:42:26Z
[Message] timestamp=2026-11-12T03:42:26Z sender=0938fee6-a149-4028-954d-d64c1f44f0ec priority=MESSAGE_PRIORITY_HIGH
**Context**: Checking on F1 investigation handoff
**Content**: Please finalize and write your findings to `e:\杂项\music-hub\.agents\teamwork\explorer_m1_1_rep\handoff.md` and send your completion message.
**Action**: Write handoff.md and reply with completion message.
