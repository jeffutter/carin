# 17: JSON API: writes and long poll

**What to build:** Outside tools can create and change things over the API. Create and edit take multipart form data: a JSON meta part (kind, format, name, role, author, session, addresses, base_revision, and on create the project fields) plus one part per file; a Section edit is a PUT with a section query and one markdown part. Delete, comment add and reply, and thread resolve and reopen complete the route table. Attribution is body fields only, defaulted as in the CLI. All core limits and embed validation apply, with errors mapped as in the reads ticket (stale base gives 409). `GET` comments with `since` and `wait` long-polls (polling the database every 500 ms, default 60 s, capped); a timeout is a plain 200 with an empty list and the same `highest_comment_id`. The OpenAPI document and snapshot are updated.

**Blocked by:** 16: JSON API: reads and OpenAPI, 07: Agent feedback loop: `--wait` and `--addresses`, 11: Markdown embeds (`cairn:` references)

**Status:** ready-for-agent

- [ ] Create, edit, section edit and delete work end to end for every Kind
- [ ] A stale `base_revision` returns 409 naming the latest Revision and its author
- [ ] Over-limit, bad Kind and bad embed return 422 with the stable code
- [ ] Long poll returns early on a new Comment and returns 200 empty on timeout
- [ ] Comment add, reply, resolve and reopen match CLI behaviour
- [ ] OpenAPI snapshot updated and passing
