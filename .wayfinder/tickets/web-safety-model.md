---
title: Web safety model for untrusted artifacts
type: grilling
status: open
assignee:
blocked_by: [research-iframe-sandbox, kinds-and-storage]
---

## Question

How does the web UI show agent-supplied HTML, SVG and images safely? Decide the iframe sandbox flags, the CSP, whether artifacts are served from a separate origin or port, how HTML-with-images resolves its images, and what the web UI itself must never trust. Record an ADR if the choice is hard to reverse.
