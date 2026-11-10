---
id: TASK-36
title: 'Fix: validate Host header on UI and content listeners (DNS rebinding)'
status: Done
assignee:
  - '@ralph'
created_date: '2026-11-10T02:01:00Z'
updated_date: '2026-11-10T02:05:00Z'
labels:
  - review-fix
  - planned
dependencies:
  - TASK-12
priority: high
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
check_post in src/ui.rs (and the UI/content routers) never validates Host. After DNS rebinding (evil.example re-resolved to 127.0.0.1:<port>), Origin==Host and a custom HX-Request header both pass, letting an attacker page read all Artifacts/Comments and post comments or resolve Threads. Add a loopback Host allowlist (127.0.0.1:<port>, localhost:<port>, [::1]) on both routers, with tests.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
SHIPPED by the TASK-36 commit. This plan is superseded; the ticket's final summary describes what actually landed.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added require_loopback_host middleware (src/serve.rs) as outermost layer on both routers; exact Host match for own port, missing Host 403. ui_router/content_router take both ports. Tests updated to send Host; new tests host_allowlist_on_both_routers and dns_rebinding_post_is_refused_and_writes_nothing. tests/cli.rs real-socket test now sends a loopback Host. ADR 0005 updated. cargo test, clippy, fmt clean.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Both listeners now 403 any request whose Host is not a loopback name for that listener's own port, closing DNS rebinding.
<!-- SECTION:FINAL_SUMMARY:END -->
