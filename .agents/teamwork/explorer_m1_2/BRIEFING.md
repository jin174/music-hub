# BRIEFING — 2026-11-12T03:25:00Z

## Mission
Investigate Feature 2 (Late SMTC Cover Byte Detection in src/monitor.rs and src/ui/mod.rs) and provide actionable implementation guidance.

## 🔒 My Identity
- Archetype: teamwork_explorer
- Roles: explorer, analyst
- Working directory: e:\杂项\music-hub
- State folder: e:\杂项\music-hub\.agents\teamwork\explorer_m1_2
- Original parent: 0938fee6-a149-4028-954d-d64c1f44f0ec
- Milestone: Milestone 1

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- Do NOT modify source files or execute build/write tasks
- Write only to state folder: e:\杂项\music-hub\.agents\teamwork\explorer_m1_2

## Current Parent
- Conversation ID: 0938fee6-a149-4028-954d-d64c1f44f0ec
- Updated: 2026-11-12T03:21:14Z

## Investigation State
- **Explored paths**:
  - `src/monitor.rs`: lines 33–88 (`CurrentPlay`, `PlaySnapshot`), 266–282 (`extract_smtc_thumbnail`), 320–352 (`tick` transition & retry), 425–475 (`start_play`)
  - `src/ui/mod.rs`: lines 100–133 (`decode_bytes_to_render_image`, `fetch_cover_render_image`), 880–900 (`now_cover_img` display), 4030–4075 (polling loop & cover key comparison)
  - `src/widget.rs`: lines 536–576 (comparison with Dynamic Island thumbnail handling)
  - `src/app.rs`: `AppStateResponse`
  - `src/config.rs`: `PublicConfig`
- **Key findings**:
  - `src/ui/mod.rs:4042` computes `format!("{}:{}", n.play_id, n.cover.len())`, omitting `cover_bytes`.
  - WinRT SMTC streams buffer asynchronously and arrive on ticks 1–2 after initial metadata.
  - Omission of `cover_bytes` causes `cur_cover != prev_key` to evaluate to `false` when bytes arrive, permanently skipping image decoding.
  - Adding `pub fn cover_key(&self) -> String` to `PlaySnapshot` returning `format!("{}:{}:{}", self.play_id, self.cover, b_len)` cleanly fixes the divergence.
- **Unexplored areas**: None for F2.

## Key Decisions Made
- Confirmed exact root cause and formulated robust key formula `format!("{}:{}:{}", n.play_id, n.cover, b_len)`.
- Recommended adding `cover_key()` method to `PlaySnapshot` in `src/monitor.rs` with dedicated zero-dependency unit test.
- Documented decoding priority and thread offloading recommendations.

## Artifact Index
- DISPATCH.md — Task instructions & received parent messages
- progress.md — Liveness heartbeat and milestone checklist
- handoff.md — 5-component handoff report for Feature 2
