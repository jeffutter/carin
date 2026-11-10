1. Submit the document and note the ref it prints (`abc123@1`):

```sh
cairn submit plan.md
```

2. Tell the human the Artifact id, then wait for their Comments. `--wait` blocks until a Comment newer than the highest one now exists, and exits 5 if nothing arrives in time; on exit 5 just wait again:

```sh
cairn comment list abc123 --wait
```

3. Change the Artifact in response. Pass the Revision you last read as `--base` so you never overwrite a newer edit (exit 6 means it moved; read it again first). Pass the root Comment ids you addressed as `--addresses`; each named Thread gets an "addressed in abc123@2" reply and stays open for the human to resolve:

```sh
cairn edit abc123 plan.md --base 1 --addresses 12,14
```

4. Remember the highest Comment id you have seen and use `--since` so each round returns only new Threads:

```sh
cairn comment list abc123 --since 14 --wait
```

For a large markdown document, edit one Section instead of resending the whole file (see "Section paths"). You can also comment yourself with `cairn comment add` and reply in a Thread with `--reply`.
