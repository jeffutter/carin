---
title: Prototype the web review UI
type: prototype
status: closed
assignee: claude
blocked_by: [revisions-and-comment-anchoring, web-safety-model, project-and-worktree-scoping]
---

## Question

What should reviewing an artifact look and feel like? Build a cheap, rough, throwaway UI (not in `src/`) to react to: the artifact list, viewing each Kind, comment placement on a Revision or markdown section, and navigating revision history. Link it as an asset; the decisions it produces land on this ticket's resolution.

## Resolution

Decided with the user by reacting to a three-variant prototype, 2026-11-09. Prototype (throwaway, fake data, open `index.html`): `.wayfinder/assets/web-ui-prototype/index.html`. It was also checked on a phone-width screen.

- **Artifact page (markdown)**: variant B. A sticky outline sidebar with open-thread counts per section; threads render inline under the section or line they anchor to; a line-number gutter and a per-heading "+ section" button start a new Thread. The sidebar has **no revision list**: the revision dropdown in the page header is the only revision navigation. The other Kinds (html, image, diagram) use the same page with the Kind's frame in place of the text, and comments on the whole Revision.
- **Artifact list**: variant A, a plain table (title, kind, project and worktree, latest Revision, open Thread count, updated), filterable by project and kind. Unscoped by default, as the scoping ticket decided.
- **Comments on other Revisions**: an open Thread made on an earlier Revision is still shown when viewing another one, with a dashed border and an "earlier revision" label. A resolved Thread from another Revision is hidden. (The prototype still shows resolved ones; this is the decision, not what the prototype does.)
- **Viewing an old Revision**: a banner says it is not the latest, with a link to the latest.
- **Responsive**: the layout collapses to one column under about 800px; the sticky sidebar becomes static. Usable on a phone.
- **Not prototyped, decided by default** (flag if wrong): the UI posts as a configured author with role reviewer, set by a flag; live refresh is htmx polling on the artifact page, not the long-poll endpoint, since the UI calls core directly (ADR 0006).
- Dropped: margin-rail artifact page (A), threads-first page (C), project-card list (B), needs-attention inbox list (C). The inbox idea may return as a later feature.
