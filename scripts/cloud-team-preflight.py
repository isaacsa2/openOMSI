#!/usr/bin/env python3
"""Read-only local checks for the fork's human-dispatched cloud workflow."""

import re
import shutil
import subprocess
import sys


def git(*args):
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, timeout=15, check=False
    )


def repository(url):
    # Accept GitHub HTTPS/SSH remotes, excluding embedded credentials and query data.
    match = re.fullmatch(
        r"(?:https://github\.com/|git@github\.com:|ssh://git@github\.com/)"
        r"([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+?)(?:\.git)?/?", url.strip()
    )
    return match.group(1).lower() if match else None


def main():
    errors = []
    root = git("rev-parse", "--show-toplevel")
    if root.returncode:
        print("FAIL: run this check inside the openOMSI Git repository.")
        return 1

    branch = git("symbolic-ref", "--quiet", "--short", "HEAD")
    branch_name = branch.stdout.strip()
    if branch.returncode or not branch_name:
        errors.append("detached HEAD; select a task branch")
    elif branch_name in {"main", "master"}:
        errors.append("main/master is not an authorized task branch")
    else:
        print(f"Branch: {branch_name}")

    for remote, expected in (
        ("origin", "isaacsa2/openomsi"),
        ("upstream", "openomsi-project/openomsi"),
    ):
        result = git("remote", "get-url", "--all", remote)
        if result.returncode or not result.stdout.strip():
            errors.append(f"missing {remote} remote")
        elif any(repository(url) != expected for url in result.stdout.splitlines()):
            errors.append(f"{remote} does not point exclusively at its expected repository")
    push = git("remote", "get-url", "--push", "--all", "origin")
    if push.returncode or not push.stdout.strip() or any(
        repository(url) != "isaacsa2/openomsi" for url in push.stdout.splitlines()
    ):
        errors.append("origin push destination is not exclusively Isaac's fork")

    if branch_name:
        tracking = git("config", "--get", f"branch.{branch_name}.remote")
        if tracking.returncode == 0 and tracking.stdout.strip() not in {"origin", "."}:
            errors.append("task branch tracks a non-fork remote; unset its upstream")
    ref = git("rev-parse", "--verify", "refs/remotes/upstream/main^{commit}")
    if ref.returncode:
        errors.append("missing upstream/main; fetch the official main before working")
    else:
        print(f"Fetched upstream main: {ref.stdout.strip()}")
        ancestry = git("merge-base", "--is-ancestor", "upstream/main", "HEAD")
        if ancestry.returncode:
            errors.append("HEAD does not prove upstream/main ancestry; refresh/deepen first")
    head = git("rev-parse", "HEAD")
    if head.returncode == 0:
        print(f"HEAD: {head.stdout.strip()}")
    status = git("status", "--porcelain")
    if status.returncode:
        errors.append("could not inspect working-tree state")
    elif status.stdout.strip():
        print("Working tree has changes: inspect the diff before handoff.")
    missing = [tool for tool in ("python3", "cargo", "rustc", "pkg-config") if not shutil.which(tool)]
    if missing:
        print("Unavailable tools (not installed by this check): " + ", ".join(missing))
    for error in errors:
        print("FAIL: " + error)
    if not errors:
        print("PASS: local repository/branch checks; remote freshness, billing and runtime remain unverified.")
    return int(bool(errors))


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, subprocess.TimeoutExpired):
        print("FAIL: Git is unavailable or a local check timed out.")
        sys.exit(1)
