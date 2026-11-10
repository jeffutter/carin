1. Submit the document. Note the printed ref (`abc123@1`). The Artifact id is the part before `@`.

```sh
cairn submit plan.md
```

2. Tell the human the Artifact id. Wait for Comments. `--wait` blocks until a Comment newer than the highest existing one arrives. Exit 5 means none arrived in time. Run the command again.

```sh
cairn comment list abc123 --wait
```

3. Edit the Artifact. Pass the Revision number you last read as `--base`. Exit 6 means a newer Revision exists. Run `cairn get abc123`, merge your change into it, and retry. Pass the root Comment ids you addressed as `--addresses`. Each named Thread gets the reply "addressed in abc123@2" and stays open for the human to resolve.

```sh
cairn edit abc123 plan.md --base 1 --addresses 12,14
```

4. Note the highest Comment id you have seen. Wait again with `--since` set to that id. Each round returns only new Threads. Repeat from step 3.

```sh
cairn comment list abc123 --since 14 --wait
```

For a large markdown document, edit one Section instead of resending the whole file (see "Section paths"). To comment yourself, run `cairn comment add`. To reply inside a Thread, add `--reply`.
