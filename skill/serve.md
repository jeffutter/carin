cairn does not start its own web UI. The human runs `cairn serve` in a terminal; ask them to if they cannot open the Artifact. Never run `cairn serve` yourself, it does not return. Print the link to an Artifact for the human with:

```sh
cairn view abc123
```

Without a running server the link does not load, so say which command to run.
