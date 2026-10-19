# Bound and expose workspace test collection

Engineering-frontier infrastructure work. The current collector captures build and
binary output and has no deadlines. A long Test job therefore does not reveal
which phase or binary is active. This is an observability/resource-control defect;
it is not evidence that the current release candidate is hung.

The successful release PR #1853 run 39239521258 built for 4m11s and executed 921
binaries for 13m46s. These are one observed run, not worst-case timing limits.

1. Change `scripts/status/collect_evidence.py`: flushed phase/binary progress,
   30-second heartbeat, bounded metadata (120s), build (45min), binary (30min),
   process-tree termination, captured failure logs. Retain ledger fields,
   attribution, missing-fixture distinction and no-fail-fast execution.
2. Add `scripts/status/test_collect_evidence.py`: synthetic success, assertion
   failure, honest fixture skip, partial success followed by timeout, descendant
   cleanup, progress, compilation failure and ledger preservation checks.
3. Wire Python checks into `.github/workflows/ci.yml`; bound Test job at 120min.
4. Run only synthetic Python tests, Ruff and whitespace checks locally; publish a
   separate CI-only PR. Do not change release files or bypass current release CI.

Budgets are generous host resource policy, not claims about emulator timing.
Windows retains its successful subprocess path and uses bounded system taskkill
for timeout cleanup; Windows cleanup remains unexecuted locally. If a Windows parent has already
exited, taskkill may not find its orphaned children. The output drain is bounded
and this case fails with cleanup explicitly unconfirmed; it is not a guarantee
of Windows orphan cleanup. Hosted collection runs on Ubuntu. No dependencies
or ledger schema changes are planned.

## Local verification

On macOS with Python 3.14.3, the following checks pass:

- `python3.14 scripts/status/test_collect_evidence.py -v`: 11 tests, including
  real subprocess timeout and POSIX descendant termination; the main-loop check
  runs a failed binary, a hung binary and a later successful binary, records one
  completed pass only, writes the ledger and returns failure.
- `ruff check scripts/status/collect_evidence.py scripts/status/test_collect_evidence.py`
- `ruff format --check scripts/status/collect_evidence.py scripts/status/test_collect_evidence.py`
- `git diff --check`

The output-drain failure case is simulated so it cannot leave a real orphan.
The Windows cleanup branch is unexecuted. No full local workspace build, emulator
execution, firmware download or release-file change was performed.

The implementation also rejects libtest exit 101 without any parsed failing
assertion, preventing an unrecognised failing output from becoming a green gate.
The ledger schema and ordinary completed-test counts remain unchanged. Timeout
logs are retained in the failing CI step; the existing ledger has no crash field,
so a deadline is not invented as a named failed test or a new evidence category.

Owning tracker: [#1856](https://github.com/emu198x/emu198x/issues/1856).
