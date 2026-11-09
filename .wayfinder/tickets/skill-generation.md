---
title: Skill generation and drift protection
type: grilling
status: open
assignee:
blocked_by: [cli-surface]
---

## Question

How does `cairn skill` produce the agent skill? Decide how clap command help is extracted (walking the clap `Command` tree at runtime), where the hand-written prose lives (embedded in the binary), the skill's output format, and how drift is caught (for example a test or CI check that the generated skill covers every command).
