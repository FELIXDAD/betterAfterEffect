# Resume notes for workstream 00 (temporary; this folder is deleted when the inventory is final)

- Part drafts live here as `<file>--<part-key>.md`. Git log tells their state:
  `spec(draft): write ... part <key>` = written, `spec(draft): verify ... part <key>` = verified,
  `spec: AE inventory <file> (verified)` = assembled into docs/spec/ae/<file>.
- Workflow script (session b7c80908): /private/tmp/claude-501/-Users-zh-orca-workspaces-betterAfterEffect-bae-00-ae-spec/b7c80908-ceac-4d48-b367-17a86a88d8bf/scratchpad/wf.js
  Research cache (~18 MB of fetched Adobe help / docsforadobe / lottie-web pages): same scratchpad, `research/` (index in research/INDEX.txt).
  Helpers: scratchpad/tools/fetch.sh (pins *.docsforadobe.dev, which the local DNS cannot resolve) and tools/commit.sh (index.lock retry).
- To resume after an interruption: keep every existing part file; re-run only the parts whose verify commit is missing
  (writers continue from existing content), then assemble, critique, fix, README.
