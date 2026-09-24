#!/usr/bin/env python3
"""Run trigger evaluation for a skill description.

Tests whether a skill's description causes the agent to trigger (load the skill)
for a set of queries. Stages the skill under .opencode/skills/, runs each query
through SKILL_EVAL_CLI (default: `opencode run --format json`), and detects
triggering from tool_use events for the skill tool. Outputs results as JSON.
"""

import argparse
import json
import os
import shlex
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path

from scripts.utils import parse_skill_md


def eval_cli() -> list:
    """Base command for eval runs from SKILL_EVAL_CLI (default: opencode run)."""
    return shlex.split(os.environ.get("SKILL_EVAL_CLI", "opencode run --format json"))


def find_project_root() -> Path:
    """Find the project root by walking up from cwd looking for .opencode/.

    The staged skill lands in <root>/.opencode/skills/ so the eval CLI
    discovers it the same way it discovers project skills.
    """
    current = Path.cwd()
    for parent in [current, *current.parents]:
        if (parent / ".opencode").is_dir():
            return parent
    return current


def stage_skill(skill_path: Path, skill_name: str, description: str, project_root: Path) -> Path:
    """Copy the skill under <root>/.opencode/skills/ with the candidate description."""
    import shutil
    dest = project_root / ".opencode" / "skills" / skill_name
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(skill_path, dest)
    skill_md = dest / "SKILL.md"
    text = skill_md.read_text()
    # Replace only the description value inside the frontmatter.
    lines = text.split("\n")
    for i, line in enumerate(lines):
        if line.startswith("description:"):
            lines[i] = "description: " + description.replace("\n", " ")
            break
    skill_md.write_text("\n".join(lines))
    return dest


def run_single_query(
    query: str,
    skill_name: str,
    timeout: int,
    project_root: str,
    model: str | None = None,
) -> bool:
    """Run a single query and return whether the skill was triggered.

    The skill must already be staged under <root>/.opencode/skills/ (see
    stage_skill). Runs SKILL_EVAL_CLI with the query on stdin and scans the
    newline-delimited JSON events for a skill-tool invocation of this skill.
    """
    cmd = eval_cli()
    if model:
        cmd.extend(["--model", model])

    try:
        proc = subprocess.run(
            cmd,
            input=query,
            capture_output=True,
            text=True,
            cwd=project_root,
            timeout=timeout,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError) as exc:
        print(f"Warning: eval CLI failed: {exc}", file=sys.stderr)
        return False

    for line in proc.stdout.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if event.get("type") != "tool_use":
            continue
        part = event.get("part", {})
        if part.get("tool") != "skill":
            continue
        state = part.get("state", {}) or {}
        tool_input = state.get("input", {}) or {}
        invoked = tool_input.get("id", "") or ""
        if not invoked:
            try:
                invoked = json.loads(part.get("rawInput", "") or "{}").get("id", "")
            except json.JSONDecodeError:
                invoked = ""
        if skill_name in invoked:
            return True
    return False


def run_eval(
    eval_set: list[dict],
    skill_name: str,
    description: str,
    num_workers: int,
    timeout: int,
    project_root: Path,
    runs_per_query: int = 1,
    trigger_threshold: float = 0.5,
    model: str | None = None,
    skill_path=None,
) -> dict:
    """Run the full eval set and return results."""
    results = []
    staged = None
    if skill_path is not None:
        staged = stage_skill(Path(skill_path), skill_name, description, Path(project_root))

    with ProcessPoolExecutor(max_workers=num_workers) as executor:
        future_to_info = {}
        for item in eval_set:
            for run_idx in range(runs_per_query):
                future = executor.submit(
                    run_single_query,
                    item["query"],
                    skill_name,
                    timeout,
                    str(project_root),
                    model,
                )
                future_to_info[future] = (item, run_idx)

        query_triggers: dict[str, list[bool]] = {}
        query_items: dict[str, dict] = {}
        for future in as_completed(future_to_info):
            item, _ = future_to_info[future]
            query = item["query"]
            query_items[query] = item
            if query not in query_triggers:
                query_triggers[query] = []
            try:
                query_triggers[query].append(future.result())
            except Exception as e:
                print(f"Warning: query failed: {e}", file=sys.stderr)
                query_triggers[query].append(False)

    for query, triggers in query_triggers.items():
        item = query_items[query]
        trigger_rate = sum(triggers) / len(triggers)
        should_trigger = item["should_trigger"]
        if should_trigger:
            did_pass = trigger_rate >= trigger_threshold
        else:
            did_pass = trigger_rate < trigger_threshold
        results.append({
            "query": query,
            "should_trigger": should_trigger,
            "trigger_rate": trigger_rate,
            "triggers": sum(triggers),
            "runs": len(triggers),
            "pass": did_pass,
        })

    passed = sum(1 for r in results if r["pass"])
    total = len(results)
    if staged is not None:
        import shutil
        shutil.rmtree(staged, ignore_errors=True)

    return {
        "skill_name": skill_name,
        "description": description,
        "results": results,
        "summary": {
            "total": total,
            "passed": passed,
            "failed": total - passed,
        },
    }


def main():
    parser = argparse.ArgumentParser(description="Run trigger evaluation for a skill description")
    parser.add_argument("--eval-set", required=True, help="Path to eval set JSON file")
    parser.add_argument("--skill-path", required=True, help="Path to skill directory")
    parser.add_argument("--description", default=None, help="Override description to test")
    parser.add_argument("--num-workers", type=int, default=10, help="Number of parallel workers")
    parser.add_argument("--timeout", type=int, default=30, help="Timeout per query in seconds")
    parser.add_argument("--runs-per-query", type=int, default=3, help="Number of runs per query")
    parser.add_argument("--trigger-threshold", type=float, default=0.5, help="Trigger rate threshold")
    parser.add_argument("--model", default=None, help="Model for the eval CLI (default: harness configured model)")
    parser.add_argument("--verbose", action="store_true", help="Print progress to stderr")
    args = parser.parse_args()

    eval_set = json.loads(Path(args.eval_set).read_text())
    skill_path = Path(args.skill_path)

    if not (skill_path / "SKILL.md").exists():
        print(f"Error: No SKILL.md found at {skill_path}", file=sys.stderr)
        sys.exit(1)

    name, original_description, content = parse_skill_md(skill_path)
    description = args.description or original_description
    project_root = find_project_root()

    if args.verbose:
        print(f"Evaluating: {description}", file=sys.stderr)

    output = run_eval(
        eval_set=eval_set,
        skill_name=name,
        description=description,
        num_workers=args.num_workers,
        timeout=args.timeout,
        project_root=project_root,
        runs_per_query=args.runs_per_query,
        trigger_threshold=args.trigger_threshold,
        model=args.model,
        skill_path=skill_path,
    )

    if args.verbose:
        summary = output["summary"]
        print(f"Results: {summary['passed']}/{summary['total']} passed", file=sys.stderr)
        for r in output["results"]:
            status = "PASS" if r["pass"] else "FAIL"
            rate_str = f"{r['triggers']}/{r['runs']}"
            print(f"  [{status}] rate={rate_str} expected={r['should_trigger']}: {r['query'][:70]}", file=sys.stderr)

    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
