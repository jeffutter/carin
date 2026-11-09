# Glossary

**Artifact**
Something an Agent submits for a Reviewer to read and discuss. It has a stable identity; its current content is its latest Revision.

**Kind**
The category an Artifact belongs to: markdown, html, image, or diagram. A diagram also has a format (mermaid or svg). Fixed when the Artifact is created.

**Revision**
An immutable, complete version of an Artifact's content, produced by each edit. Numbered 1, 2, 3 within its Artifact. Revisions are never changed after creation.

**Bundle**
The set of files that makes up one html Revision: an entry page plus its images and other assets.

**Embed**
A markdown Artifact showing an image or diagram Artifact by pointing at it. The markdown itself holds no image files.

**Comment**
A remark on one Revision of an Artifact, optionally pointing at a part of it (a line range, or for markdown a section). Written by a Reviewer or an Agent. Never edited after posting.

**Section**
A part of a markdown Artifact from one heading to the next heading of the same or higher level, children included. Named by a path of heading texts.

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
