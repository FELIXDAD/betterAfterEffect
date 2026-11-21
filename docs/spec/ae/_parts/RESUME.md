# Resume notes for workstream 00 (temporary; this folder is deleted when the inventory is final)

- Part drafts live here as `<file>--<part-key>.md`. Git log tells their state:
  `spec(draft): write ... part <key>` = written, `spec(draft): verify ... part <key>` = verified,
  `spec: AE inventory <file> (verified)` = assembled into docs/spec/ae/<file>.
- Workflow script (session b7c80908): /private/tmp/claude-501/-Users-zh-orca-workspaces-betterAfterEffect-bae-00-ae-spec/b7c80908-ceac-4d48-b367-17a86a88d8bf/scratchpad/wf.js
  Research cache (~18 MB of fetched Adobe help / docsforadobe / lottie-web pages): same scratchpad, `research/` (index in research/INDEX.txt).
  Helpers: scratchpad/tools/fetch.sh (pins *.docsforadobe.dev, which the local DNS cannot resolve) and tools/commit.sh (index.lock retry).
- To resume after an interruption: keep every existing part file; re-run only the parts whose verify commit is missing
  (writers continue from existing content), then assemble, critique, fix, README.

## Phase 2 (session b7c80908, 14:10 CET 2026-11-20)
- Run 1 (wf.js) wrote prj-a, ani-b, cmp-a, cmp-b, shp, txt fully and prj-b, ani-a, fx-a, ui-a partially, then hit the usage limit.
- Phase-2 script: scratchpad/wf2.js, data-driven via args
  `{write:[...], verify:[...], assemble:[...], gated:[...], critique, readme}`; verifiers checkpoint in
  scratchpad/research/<key>/verify-progress.md and commit `spec(draft): verifying part <key> (through <ID>)`.
  To resume: derive the remaining keys from git log and pass them as args.
- 14:35 CET: PRJ, ANI, CMP, SHP, TXT final files verified and assembled (they equal their parts). Running run
  wf_bf2edcf7-19b with args {write/verify: fx-a..c, ui-a..c; assemble: effects.md, ui-preview-render-io.md; gated: []},
  then critique -> fix (edits FINAL files, parts become stale) -> README. Note: never resume a run whose verifier
  prompts embed writer JSON (cache keys change); start a fresh run with narrowed args instead.
