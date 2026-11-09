# Glossary

**Artifact**
Something an Agent submits for a Reviewer to read and discuss. It has a stable identity; its current content is its latest Revision.

**Kind**
The category an Artifact belongs to: markdown, html, image, or diagram. _(Definition still loose: the exact set and boundaries are undecided.)_

**Revision**
An immutable, complete version of an Artifact's content, produced by each edit. Numbered 1, 2, 3 within its Artifact. Revisions are never changed after creation.

**Comment**
A remark on one Revision of an Artifact, optionally pointing at a part of it (a line range, or for markdown a section). Written by a Reviewer or an Agent. Never edited after posting. _(Section addressing is still undecided.)_

**Thread**
A root Comment plus the replies to it. Either party may reply. A Thread is open or resolved, and either party may change that.

**Author**
Who wrote a Revision or Comment: a free-text name plus a role (agent or reviewer). A label, not a verified identity.

**Session**
An Agent's working session, recorded as an opaque label on its Revisions and Comments so an Agent can find what it produced. Used for filtering, never for access control.

**Agent**
The automated party that submits, retrieves, edits and deletes Artifacts, and reads and replies to Comments.

**Reviewer**
The human who views, lists, comments on and deletes Artifacts.
