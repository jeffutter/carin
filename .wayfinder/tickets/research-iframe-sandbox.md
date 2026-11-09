---
title: Research iframe sandboxing and CSP for untrusted agent HTML
type: research
status: open
assignee: jeffutter
blocked_by: []
---

## Question

What is current best practice for showing untrusted HTML (single page, with images) inside a web app? Cover the iframe `sandbox` attribute combinations, `srcdoc` versus a URL, CSP (including `sandbox` and `default-src` via header), serving from a separate origin or port on localhost, and which of these an axum server can do. Surface facts only; the decision is the Web safety model ticket.
