#!/usr/bin/env python3
"""Compare two openOMSI schema-1 performance captures.

This is intentionally a report/comparison tool, not a benchmark runner. The captures
must come from the same machine, scene, camera and settings for a regression claim.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

LATENCY_METRICS = ("p50_ms", "p95_ms", "p99_ms", "worst_frame_ms")
FPS_METRIC = "average_fps"


def number(value: Any, name: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ValueError(f"{name} is not numeric")
    value = float(value)
    if not math.isfinite(value):
        raise ValueError(f"{name} is not finite")
    return value


def load_capture(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"{path}: {exc}") from exc
    if value.get("schema_version") != 1:
        raise ValueError(f"{path}: unsupported schema_version {value.get('schema_version')!r}; expected 1")
    summary = value.get("summary")
    if not isinstance(summary, dict):
        raise ValueError(f"{path}: missing summary object")
    for metric in (FPS_METRIC, *LATENCY_METRICS):
        number(summary.get(metric), f"{path}:{metric}")
    stages = summary.get("stage_average_ms", {})
    if not isinstance(stages, dict):
        raise ValueError(f"{path}: summary.stage_average_ms is not an object")
    for key, val in stages.items():
        number(val, f"{path}:stage_average_ms.{key}")
    return value


def first_graphics(capture: dict[str, Any]) -> dict[str, Any] | None:
    frames = capture.get("frames")
    if not isinstance(frames, list):
        return None
    for frame in frames:
        if isinstance(frame, dict) and isinstance(frame.get("graphics"), dict):
            return frame["graphics"]
    return None


def percent_change(base: float, candidate: float) -> float | None:
    if base == 0:
        return None
    return (candidate - base) / abs(base) * 100.0


def regression_percent(metric: str, base: float, candidate: float) -> float:
    if base <= 0:
        return 0.0
    if metric == FPS_METRIC:
        return max(0.0, (base - candidate) / base * 100.0)
    return max(0.0, (candidate - base) / base * 100.0)


def fmt_change(change: float | None) -> str:
    return "n/a" if change is None else f"{change:+.1f}%"


def compare(base: dict[str, Any], candidate: dict[str, Any]) -> dict[str, Any]:
    bs = base["summary"]
    cs = candidate["summary"]
    metrics: dict[str, dict[str, float | None]] = {}
    for metric in (FPS_METRIC, *LATENCY_METRICS):
        b = number(bs[metric], metric)
        c = number(cs[metric], metric)
        metrics[metric] = {
            "base": b,
            "candidate": c,
            "change_percent": percent_change(b, c),
            "regression_percent": regression_percent(metric, b, c),
        }

    b_stages = bs.get("stage_average_ms", {})
    c_stages = cs.get("stage_average_ms", {})
    stages = []
    for name in sorted(set(b_stages) & set(c_stages)):
        b = number(b_stages[name], name)
        c = number(c_stages[name], name)
        stages.append(
            {
                "name": name,
                "base_ms": b,
                "candidate_ms": c,
                "delta_ms": c - b,
                "change_percent": percent_change(b, c),
            }
        )
    stages.sort(key=lambda row: row["delta_ms"], reverse=True)

    mismatches = []
    for key in ("requested_seconds", "warmup_seconds"):
        if base.get(key) != candidate.get(key):
            mismatches.append(f"{key}: {base.get(key)!r} -> {candidate.get(key)!r}")
    bg = first_graphics(base)
    cg = first_graphics(candidate)
    if bg != cg:
        mismatches.append(f"graphics: {bg!r} -> {cg!r}")

    return {"metrics": metrics, "stages": stages, "mismatches": mismatches}


def print_report(result: dict[str, Any], max_stages: int) -> None:
    print("| Metric | Base | Candidate | Change |")
    print("| --- | ---: | ---: | ---: |")
    labels = {
        FPS_METRIC: "Average FPS",
        "p50_ms": "p50 frame time (ms)",
        "p95_ms": "p95 frame time (ms)",
        "p99_ms": "p99 frame time (ms)",
        "worst_frame_ms": "Worst frame (ms)",
    }
    for metric in (FPS_METRIC, *LATENCY_METRICS):
        row = result["metrics"][metric]
        print(
            f"| {labels[metric]} | {row['base']:.3f} | {row['candidate']:.3f} | "
            f"{fmt_change(row['change_percent'])} |"
        )
    stages = result["stages"][: max(0, max_stages)]
    if stages:
        print()
        print("| Stage | Base ms | Candidate ms | Delta ms | Change |")
        print("| --- | ---: | ---: | ---: | ---: |")
        for row in stages:
            print(
                f"| `{row['name']}` | {row['base_ms']:.3f} | {row['candidate_ms']:.3f} | "
                f"{row['delta_ms']:+.3f} | {fmt_change(row['change_percent'])} |"
            )
    if result["mismatches"]:
        print()
        print("Comparison warnings:")
        for item in result["mismatches"]:
            print(f"- {item}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--json", action="store_true", dest="json_output",
                        help="emit machine-readable comparison JSON")
    parser.add_argument("--max-stage-rows", type=int, default=15)
    parser.add_argument("--allow-mismatch", action="store_true",
                        help="allow capture duration/warmup/graphics metadata to differ")
    parser.add_argument("--fail-p95-percent", type=float)
    parser.add_argument("--fail-p99-percent", type=float)
    parser.add_argument("--fail-fps-drop-percent", type=float)
    args = parser.parse_args(argv)

    try:
        base = load_capture(args.base)
        candidate = load_capture(args.candidate)
        result = compare(base, candidate)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.json_output:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print_report(result, args.max_stage_rows)

    failed = False
    if result["mismatches"] and not args.allow_mismatch:
        print("error: captures differ in comparison metadata; use --allow-mismatch only when intentional",
              file=sys.stderr)
        failed = True
    checks = [
        ("p95_ms", args.fail_p95_percent, "p95 frame-time regression"),
        ("p99_ms", args.fail_p99_percent, "p99 frame-time regression"),
        (FPS_METRIC, args.fail_fps_drop_percent, "average FPS drop"),
    ]
    for metric, limit, label in checks:
        if limit is None:
            continue
        if limit < 0:
            print(f"error: threshold for {label} must be >= 0", file=sys.stderr)
            return 2
        regression = result["metrics"][metric]["regression_percent"]
        if regression > limit:
            print(f"error: {label} {regression:.1f}% exceeds {limit:.1f}%", file=sys.stderr)
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
