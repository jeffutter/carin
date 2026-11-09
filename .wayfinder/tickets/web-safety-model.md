---
title: Web safety model for untrusted artifacts
type: grilling
status: open
assignee:
blocked_by: [research-iframe-sandbox, kinds-and-storage]
---

## Question

How does the web UI show agent-supplied HTML, SVG and images safely? Decide the iframe sandbox flags, the CSP, whether artifacts are served from a separate origin or port, how HTML-with-images resolves its images, and what the web UI itself must never trust. Record an ADR if the choice is hard to reverse.

## Carried in from Artifact kinds and storage

Blobs live in sqlite with a stored media type; decide how they are served from the untrusted origin (Content-Type from an allowlist, nosniff). Html Bundles serve assets by relative path. Markdown `cairn:` embeds (image and diagram Artifacts only) render as `<img>` from the untrusted origin; `http:`, `data:` and relative image URLs in markdown are not rendered.
