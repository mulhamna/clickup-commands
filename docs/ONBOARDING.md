# Onboarding Claude Code

How to kick off implementation with Claude Code in this repo.

## 1. Prepare the repo

```bash
gh repo create mulhamna/clickup-commands --public --clone
cd clickup-commands
# copy these docs in: README.md, CLAUDE.md, AGENTS.md, docs/*
git add . && git commit -m "docs: add design docs and agent guide"
git push -u origin main

# reference repo, read-only, next to this one
git clone --depth 1 https://github.com/mulhamna/jira-commands ../jira-commands
```

Get a personal token for manual testing later (ClickUp → avatar → Settings → Apps → API Token). Don't paste it into Claude; export it in your shell when you test: `export CLICKUP_TOKEN=pk_…`.

## 2. First prompt (Phase 0 + 1)

Paste into Claude Code:

```text
Read CLAUDE.md, then docs/ROADMAP.md, docs/ARCHITECTURE.md, docs/CLICKUP-API.md and
docs/JIRA-MAPPING.md. The reference implementation is at ../jira-commands — read its
crates/jira-core/src/{config,error,client}.rs, crates/jira-mcp/src/{lib,error}.rs and
crates/jira-mcp/src/app/shared.rs before writing the equivalents here.

Implement Phase 0, then stop and show me the workspace layout and smoke-test output.
After I confirm, implement Phase 1 task by task, committing each task with a
Conventional Commit. Don't call the real ClickUp API from tests; use wiremock.
Before using any endpoint marked (verify) in docs/CLICKUP-API.md, check it against
https://developer.clickup.com (index: https://developer.clickup.com/llms.txt) and
remove the marker. At the end of Phase 1, run the smoke test and report what's
done, what deviated from the docs, and anything I need to test manually.
```

## 3. Next phases

```text
Phase N of docs/ROADMAP.md. Same rules as before: read the relevant sections of
docs/CLI.md and docs/MCP.md first, mirror jira-commands' patterns, wiremock tests
for every client method (happy path, 404, 429-then-success, 429 exhausted), verify
(verify) endpoints, commit per task, smoke test green, then report.
```

## 4. Manual checks the owner does after each phase

- Phase 1: `clickupc auth login`, `clickupc tree`, `clickupc list view <list>`.
- Phase 2: create → update → status → move → delete a task in a scratch list; run `clickupc mcp serve` from Claude Code and ask it to list your tasks.
- Phase 3: add a comment, upload a file, set a dropdown custom field by option name, start/stop a timer.
- Phase 4: `task bulk update --dry-run` on a filter, then for real on a scratch list.
- Phase 5: `clickupc mcp install --client claude-code`, restart Claude Code, check tools appear.

## 5. Keeping docs honest

When implementation deviates from these docs (an endpoint behaves differently, a field is named differently), Claude should update the doc in the same commit (`docs:` type is fine in a `feat:` PR). The docs are the spec for the next phase.
