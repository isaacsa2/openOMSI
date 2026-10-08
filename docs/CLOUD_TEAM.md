# Cloud development coordination / Coordenação de desenvolvimento em nuvem

## Scope and current status

Fork: `isaacsa2/openOMSI`. Upstream: `openOMSI-Project/openOMSI`.
Initial upstream and fork main: `c85b2e0dda13d219fd12d0496c653d20cdb54145`.
The configuration lives on `chore/cloud-team-setup` and `team/cloud-base`;
main is unchanged. Do not merge the setup PR merely to activate the instructions.
Start sessions on these branches so the files are actually present.

GitHub read access and local clone: verified in Work on 2026-10-08.
Claude account, repository authorization, cloud VM, tests and GitHub round trip:
pending. The browser reached Claude sign-in, then displayed an hCaptcha after
Google was selected. No authenticated Claude Code session has been verified.
No automated bridge, background watcher, API keys or paid services are configured.
Update this status only after observing real results.

## Roles and collaboration channel

Isaac approves main changes, releases and upstream submissions.
Work coordinates scopes, implements tasks and reviews Claude changes.
Claude implements a different claimed task or reviews Work changes.
Swap implementer and reviewer for each task as useful. Do not approve your own
change as though a second agent reviewed it. Both may post through Isaac's GitHub
identity: distinguish the originating session in the handoff, without inventing
independent GitHub approvals. No shared terminal or implicit shared memory exists.

Issues are disabled in this fork. Use the draft setup PR's Conversation as the
task ledger, and a separate draft PR for each implementation. Do not enable
issues or install event-triggered automation merely to make this protocol work.
Starting a task or review still requires a user/Work dispatch. A GitHub comment
does not by itself start either service. Work can dispatch through the cloud
browser only while its authenticated Claude session remains available.

## Task lifecycle

Use: queued -> claimed -> in progress -> ready for review -> changes requested
or reviewed -> awaiting Isaac. Use blocked when access, tools or tests are missing.

Post this claim before changing files, then reread recent comments for conflicts:

```text
Task: T-001 / one concrete reason
State: claimed
Implementer: Work or Claude / session link if safe
Reviewer: the other tool / not yet dispatched
Branch: work/T-001-short-reason or claude/T-001-short-reason
Base: full latest upstream SHA + baseline SHA
Scope: files/subsystems and acceptance criteria
Tests: planned commands and platforms
```

One owner edits a branch. Overlapping files require sequential work or agreement
in the ledger. Independent tasks use isolated branches. If two claims race,
pause the later claim and resolve the overlap; GitHub comments are not atomic locks.
Use Claude's generated `claude/...` branch if its service requires that prefix.

For review, hand over a draft PR URL and full head SHA. The other agent fetches
that exact commit, reads the diff, runs relevant checks and posts findings tied
to that SHA. If its GitHub session cannot post, provide the review in the Claude
session and relay it verbatim with its origin identified; mark direct write as
unverified. Never turn a relayed message into a fabricated GitHub review approval.
After fixes, request a review of the new SHA. Do not auto-merge.

## Baseline and branches

The baseline carries these fork-only instructions plus current upstream code.
It must not accumulate unrelated game fixes. Before a new implementation:

1. Verify `origin` is Isaac's fork and `upstream` is the official repository.
2. Fetch both, including their main refs and `team/cloud-base`.
3. Inspect upstream open PRs and relevant fork PRs to avoid duplicates.
4. Create `chore/cloud-base-update-<short-sha>` from `origin/team/cloud-base`;
   merge `upstream/main` there if the baseline is behind. Resolve conflicts with
   narrow changes and validate. Open a draft update PR targeting `team/cloud-base`.
   Refresh the baseline after that update has been reviewed. Do not alter main.
5. Create each task branch from the refreshed `origin/team/cloud-base`, which
   must contain latest `upstream/main`. Run the preflight before editing or pushing.

```sh
git fetch --no-tags origin
git fetch --no-tags upstream main
git switch -c work/T-001-short-reason origin/team/cloud-base
git branch --unset-upstream
python3 scripts/cloud-team-preflight.py
# after tests and reviewable changes:
git push origin HEAD:refs/heads/work/T-001-short-reason
```

Fetch enough history to establish ancestry if the clone is shallow. Do not interpret
an unknown ancestry check as proof that the baseline is current. Never force-push
a shared branch or use a bare `git push` with an upstream tracking destination.
Implementation PRs target `team/cloud-base`; the setup PR targets main for inspection
only and stays a draft. Before any authorized upstream submission, prepare a clean
branch from latest upstream containing only that task, excluding fork-only setup.

`cloud-team-preflight.py` is a read-only check of local refs, remote destinations
and branch state. It performs no fetch/push and cannot enforce GitHub permissions.
It rejects main, detached HEAD, an upstream push destination and missing/stale
upstream ancestry. It lists unavailable development tools without installing them.
It does not inspect or print credentials and does not prove subscription billing.

## Environments, access and cost

Work already has a working clone and the connected GitHub plugin. This does not
establish that a reusable Codex Cloud environment has been published. If needed,
create/select it through the web environment setup, using only this fork.
For Claude: sign in with the existing Pro account, select this fork and the
setup branch, then create a cloud environment using the included plan allocation.
Repository authorization may require Isaac to approve the Claude GitHub App;
choose only this repository and inspect the requested permissions. Do not grant
unrelated repositories, paste GitHub tokens into prompts, or enable extra usage.

Follow the Linux dependencies already listed in `.github/workflows/release.yml`
and `scripts/build-linux.sh`, and the existing stable Rust toolchain. These
instructions do not install anything or enable an environment automatically.
Use trusted package/GitHub access only as required for the existing dependencies;
do not weaken the environment's network or permission controls.
If the environment cannot install tools, record the exact blocker and use existing
CI. No separate API invocation is needed for the human-dispatched workflow.
Do not set model API keys, use API-funded GitHub Actions for agents, activate
Claude Code Review billing, purchase credits or upgrade a plan. At a usage limit,
stop and wait for the plan allocation to reset. Do not activate auto-fix/watchers
until separately requested and their access/cost is checked.

## Validation

For this setup: syntax-check the Python helper, run it on the setup branch,
exercise its main/stale/detached/incorrect-remote rejection in disposable repos,
and run `git diff --check`. No application code or workflows are changed.
For Rust tasks: scoped regression tests first, followed by the workspace tests,
release build and format check in `AGENTS.md`. Use existing platform CI and record
run links and statuses. A check still running is pending, not passed. Do not wait
indefinitely: return after one status check, with at most 15 minutes of active
monitoring when explicitly needed.
For larger compatibility changes compare `omsi-check` before/after with local
`OMSI_ROOT`. Without legitimate content, report that compatibility test as skipped.
Windows, Android, graphics drivers, controls and in-game behavior require their
real platform/tester checks; cloud Linux does not certify those.

## Integration round-trip acceptance test

1. Work publishes the setup branch and a draft PR only in the fork, then reads
   them back from GitHub. Record setup SHA and unchanged main SHA.
2. In authenticated Claude Code Web, select the fork and setup branch. Dispatch
   the prompt below. Verify the cloud task starts and reads the exact setup SHA.
3. Claude reports an independent review and actual check output. If its connection
   permits comments, post a review comment to the setup PR with that SHA.
4. Work reads the comment from GitHub and verifies its session origin and SHA;
   addresses findings on the setup branch, then requests rereview if needed.
5. Only then report which paths passed: repository read, cloud execution, direct
   GitHub comment or relayed review. Never call a relay an automatic integration.

### First Claude task (replace the PR URL and SHA with verified values)

```text
Review the cloud-team setup in isaacsa2/openOMSI at SETUP_SHA on
chore/cloud-team-setup. Read CLAUDE.md, AGENTS.md, CONTRIBUTING.md and
docs/CLOUD_TEAM.md. Use SETUP_PR as the coordination thread. Do not edit files,
push, merge, create PRs, change main, contact upstream or enable paid services.
Verify HEAD and the diff against upstream/main. Run the Python preflight and
git diff --check for the setup commit. Review task ownership, upstream sync,
cross-review, tests and the absence of paid API integration. Report commands,
results and findings tied to the full SHA. If GitHub commenting is available,
post one ordinary review comment on SETUP_PR; otherwise return the review here
and explicitly state that GitHub write access was not verified. Use only the
existing Pro allocation; stop at limits instead of enabling paid extra usage.
```

## Referências oficiais / Official references

- https://learn.chatgpt.com/docs/cloud
- https://learn.chatgpt.com/docs/agent-configuration/agents-md
- https://code.claude.com/docs/en/claude-code-on-the-web
- https://code.claude.com/docs/en/memory
- https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan
