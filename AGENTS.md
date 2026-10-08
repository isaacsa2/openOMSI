# Fork development rules

Read `CONTRIBUTING.md` and `docs/CLOUD_TEAM.md` before starting a task.

- Work only in `isaacsa2/openOMSI`. Isaac must explicitly authorize any write,
  merge or push to `main`, release, or upstream pull request. No auto-merge.
- One concrete reason per task, branch and draft PR. Search both repositories'
  open PRs before implementing. Preserve existing architecture and compatibility.
- The shared baseline is `team/cloud-base`. Before implementation, fetch the latest
  `openOMSI-Project/openOMSI` main, integrate it into the baseline on a separate
  update branch, and report conflicts. Never synchronize by changing fork main.
- Create task branches from the refreshed baseline. Target draft PRs at
  `team/cloud-base`, never at upstream. Push with an explicit fork remote and ref.
- Claim a task in the coordination PR with scope, owner, branch, base SHA and
  affected files; reread claims and avoid overlapping edits. Claims are advisory,
  not locks. Work and Claude sessions have separate filesystems and context.
- Use GitHub PR conversations for handoffs. Work implements and Claude reviews,
  or Claude implements and Work reviews. A review records the exact head SHA,
  findings and commands actually run. A later push invalidates the old review.
- Follow the existing CI. For Rust changes run scoped tests, then
  `cargo test --locked --workspace --no-fail-fast` and `cargo build --locked --release`.
  Run `cargo fmt --all -- --check`. Record missing tools/content and platform tests
  still pending. A green build alone does not demonstrate in-game behavior.
- Do not poll builds indefinitely. Check once and return the run URL if pending;
  any active CI monitoring must end within 15 minutes. Never claim unrun tests pass.
- Use existing subscriptions only. Do not call separately billed model APIs,
  buy credits, enable paid extra usage, or provision paid services. Stop at limits.
- No original OMSI content, credentials, caches or build artifacts in commits.
  Write PR descriptions PT-BR first, then English; keep upstream issues English.
- These are project instructions, not a server-enforced branch protection rule.
  Do not claim the two services are connected until the round-trip test passes.
