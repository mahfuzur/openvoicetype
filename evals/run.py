#!/usr/bin/env python3
"""Formatting-quality eval for the dictation pipeline.

Default: feeds each case's `input` (a realistic raw Whisper transcript) to `dictate.sh refine`,
which tests the cleanup layer on its own. With --e2e: synthesizes the case's `say` text with macOS `say`,
then runs `dictate.sh transcribe` + `refine`, which tests Whisper, cleanup and post-processing together.

Usage:
  evals/run.py [--model haiku|sonnet] [--cleanup claude|s1] [--case ID ...] [--e2e] [--runs N] [--jobs N] [--show]

--cleanup s1 evaluates S1-mini (offline, through llama-server) instead of Claude.
"""
import argparse
import concurrent.futures as futures
import json
import os
import re
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "dictate.sh"
CASES = ROOT / "evals" / "cases.json"
RESULTS = ROOT / "evals" / "results"

LIST_LINE = re.compile(r"^\s*(?:[-*•]|\d+[.)])\s+\S", re.MULTILINE)
SENTENCE_END = re.compile(r"[.!?](?=\s|$)")


def run_script(args, text=None, env=None, timeout=90):
    result = subprocess.run(
        ["/bin/bash", str(SCRIPT), *args],
        input=text, capture_output=True, text=True, env=env, timeout=timeout,
    )
    return result.returncode, result.stdout.strip()


def case_env(case, args, log_file):
    env = dict(os.environ)
    env.update({
        "VTT_QUIET": "on",
        "VTT_REFINE": "on",
        "VTT_CLEANUP": args.cleanup,
        # Measure the selected engine alone: a Claude failure must not be hidden by the S1-mini fallback.
        "VTT_S1_FALLBACK": "off",
        "VTT_CLAUDE_MODEL": args.model,
        "VTT_MODE": case.get("mode", "default"),
        "VTT_APP": case.get("app", ""),
        "VTT_VOCAB": ", ".join(case.get("vocabulary", [])),
        "VTT_LOG_FILE": str(log_file),
        # Parallel eval calls are slower than a single dictation; don't let the 15 s default fall back to raw.
        "CLAUDE_TIMEOUT": "45",
        # Evaluate the repo's prompts, not a personal override or dictionary.
        "PROMPT_FILE": "/nonexistent",
        "DICTIONARY_FILE": "/nonexistent",
    })
    return env


def synthesize(text, directory):
    aiff = Path(directory) / "speech.aiff"
    wav = Path(directory) / "speech.wav"
    subprocess.run(["say", "-o", str(aiff), text], check=True)
    subprocess.run(["sox", str(aiff), "-r", "16000", "-c", "1", "-b", "16", str(wav)], check=True)
    return wav


def not_contains_pattern(term):
    """Case-insensitive; word boundaries only on sides that start/end with a word character."""
    left = r"\b" if re.match(r"\w", term) else ""
    right = r"\b" if re.search(r"\w$", term) else ""
    return re.compile(left + re.escape(term) + right, re.IGNORECASE)


def check(output, checks):
    failures = []
    for term in checks.get("contains", []):
        if term not in output:
            failures.append(f"missing {term!r}")
    for term in checks.get("not_contains", []):
        if not_contains_pattern(term).search(output):
            failures.append(f"should not contain {term!r}")
    for pattern in checks.get("regex", []):
        if not re.search(pattern, output):
            failures.append(f"no match for /{pattern}/")

    list_items = len(LIST_LINE.findall(output))
    if "min_list_items" in checks and list_items < checks["min_list_items"]:
        failures.append(f"{list_items} list items, want >= {checks['min_list_items']}")
    if "max_list_items" in checks and list_items > checks["max_list_items"]:
        failures.append(f"{list_items} list items, want <= {checks['max_list_items']}")

    paragraphs = [p for p in re.split(r"\n\s*\n", output) if p.strip()]
    if "min_paragraphs" in checks and len(paragraphs) < checks["min_paragraphs"]:
        failures.append(f"{len(paragraphs)} paragraphs, want >= {checks['min_paragraphs']}")
    if "max_sentences_per_paragraph" in checks:
        longest = max((len(SENTENCE_END.findall(p)) for p in paragraphs), default=0)
        if longest > checks["max_sentences_per_paragraph"]:
            failures.append(f"a paragraph has {longest} sentences, want <= {checks['max_sentences_per_paragraph']}")
    return failures


def run_case(case, args, log_file):
    env = case_env(case, args, log_file)
    started = time.time()
    raw = case["input"]
    if args.e2e:
        if "say" not in case:
            return {"id": case["id"], "skipped": True}
        with tempfile.TemporaryDirectory() as directory:
            wav = synthesize(case["say"], directory)
            _, raw = run_script(["transcribe", str(wav)], env=env)
    code, output = run_script(["refine"], text=raw, env=env)
    elapsed = time.time() - started
    failures = check(output, case.get("checks", {}))
    if code == 3:
        failures.insert(0, "cleanup failed (fell back to raw text)")
    return {"id": case["id"], "mode": case.get("mode", "default"), "raw": raw, "output": output,
            "seconds": round(elapsed, 2), "failures": failures, "passed": not failures}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--model", default="haiku", help="Claude model (with --cleanup claude)")
    parser.add_argument("--cleanup", default="claude", choices=["claude", "s1"])
    parser.add_argument("--case", action="append", help="run only these case ids")
    parser.add_argument("--e2e", action="store_true", help="synthesize speech and run Whisper too")
    parser.add_argument("--runs", type=int, default=1, help="repeat each case (flakiness check)")
    parser.add_argument("--jobs", type=int, default=0, help="parallel cases (default 4, or 2 with --e2e)")
    parser.add_argument("--show", action="store_true", help="print every output, not just failures")
    args = parser.parse_args()

    cases = json.loads(CASES.read_text())
    if args.case:
        cases = [c for c in cases if c["id"] in args.case]
    jobs = args.jobs or (2 if args.e2e else 4)
    engine = args.model if args.cleanup == "claude" else "s1"
    if args.cleanup == "s1" and run_script(["s1-server", "start"])[0] != 0:
        sys.exit("S1-mini is not installed or did not start (run scripts/install.sh)")

    RESULTS.mkdir(parents=True, exist_ok=True)
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    label = f"{stamp}-{engine}{'-e2e' if args.e2e else ''}"
    log_file = RESULTS / f"{label}.log"

    work = [case for case in cases for _ in range(args.runs)]
    with futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        results = list(pool.map(lambda c: run_case(c, args, log_file), work))
    results = [r for r in results if not r.get("skipped")]

    width = max((len(r["id"]) for r in results), default=10)
    for r in results:
        mark = "PASS" if r["passed"] else "FAIL"
        print(f"{mark}  {r['id']:<{width}}  {r['seconds']:>5.1f}s  {r['mode']}")
        if not r["passed"] or args.show:
            for failure in r["failures"]:
                print(f"        - {failure}")
            if args.e2e:
                print("        raw:    " + r["raw"].replace("\n", "\n                "))
            print("        output: " + r["output"].replace("\n", "\n                "))

    passed = sum(r["passed"] for r in results)
    seconds = [r["seconds"] for r in results]
    median = statistics.median(seconds) if seconds else 0
    print(f"\n{passed}/{len(results)} passed ({100 * passed / max(len(results), 1):.0f}%)  "
          f"engine={engine}  median={median:.1f}s{'  e2e' if args.e2e else ''}")

    report = RESULTS / f"{label}.json"
    report.write_text(json.dumps({"model": engine, "e2e": args.e2e, "passed": passed,
                                  "total": len(results), "median_seconds": median,
                                  "results": results}, indent=2))
    print(f"report: {report.relative_to(ROOT)}")
    sys.exit(0 if passed == len(results) else 1)


if __name__ == "__main__":
    main()
