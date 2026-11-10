Only the human starts the web UI, by running `cairn serve` in a terminal. Ask them to run it when they cannot open the Artifact. Do not run `cairn serve` yourself, because it never returns. Print the link to an Artifact:

```sh
cairn view abc123
```

The link loads only while `cairn serve` runs. Tell the human to run `cairn serve` when it does not load.
