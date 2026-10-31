"""Compile and run Xinyi's host-side Rust tests; no NASM, QEMU or disk images required."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
# These files are modules of storage_tests.rs, not independent Rust test crates.
INCLUDED = {name: 'storage' for name in ('folder', 'fsck', 'stream', 'stream_fault')}


def suites():
    found = {p.name.removesuffix('_tests.rs'): p for p in (ROOT / 'tools').glob('*_tests.rs')}
    for child, parent in INCLUDED.items():
        if child not in found or parent not in found:
            raise RuntimeError(f'Missing test crate/module: {child} -> {parent}')
    return {name: path for name, path in sorted(found.items()) if name not in INCLUDED}


def execute(command, log, timeout):
    with log.open('w', encoding='utf-8') as stream:
        stream.write(subprocess.list2cmdline(command) + '\n\n')
        stream.flush()
        try:
            result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                    timeout=timeout, check=False)
            return result.returncode, False
        except subprocess.TimeoutExpired:
            stream.write(f'\nTIMEOUT after {timeout} seconds\n')
            return None, True
        except OSError as error:
            stream.write(f'\nCould not execute command: {error}\n')
            return None, False


def run_suite(name, path, output, rustc, timeout):
    start = time.monotonic()
    binary = output / (name + ('-tests.exe' if os.name == 'nt' else '-tests'))
    compile_log, test_log = output / f'{name}-compile.log', output / f'{name}-test.log'
    code, timed_out = execute([rustc, '--edition=2021', '--test', str(path), '-o', str(binary)], compile_log, timeout)
    result = dict(suite=name, source=str(path.relative_to(ROOT)), status='failed', phase='compile',
                  returncode=code, timed_out=timed_out, compile_log=str(compile_log))
    if code == 0:
        code, timed_out = execute([str(binary), '--color', 'never'], test_log, timeout)
        result.update(phase='test', returncode=code, timed_out=timed_out, test_log=str(test_log))
        match = re.search(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', test_log.read_text(encoding='utf-8', errors='replace'))
        if match:
            result.update(passed=int(match[2]), failed=int(match[3]), ignored=int(match[4]))
            if code == 0 and match[1] == 'ok' and int(match[3]) == 0:
                result['status'] = 'passed'
        elif code == 0:
            result['error'] = 'Test process succeeded without a recognised Rust test summary'
    result['seconds'] = round(time.monotonic() - start, 3)
    return result


def main():
    available = suites()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', nargs='+', choices=sorted(set(available) | set(INCLUDED)), help='Run selected suites; storage module names run the complete storage crate')
    parser.add_argument('--list', action='store_true', help='List suites without compiling')
    parser.add_argument('--jobs', type=int, default=2, help='Concurrent suites (default: 2)')
    parser.add_argument('--timeout', type=float, default=300, help='Seconds allowed per compile/test process (default: 300)')
    parser.add_argument('--rustc', default='rustc', help='Rust compiler executable')
    args = parser.parse_args()
    if args.jobs < 1 or args.timeout <= 0:
        parser.error('--jobs and --timeout must be positive')
    selected = sorted({INCLUDED.get(name, name) for name in (args.suite or available)})
    if args.list:
        for name in selected:
            children = [child for child, parent in INCLUDED.items() if parent == name]
            print(name + (f' (includes {", ".join(children)})' if children else ''))
        return 0
    try:
        compiler = subprocess.run([args.rustc, '-vV'], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        parser.exit(2, f'Cannot run Rust compiler {args.rustc!r}: {error}\n')
    base = ROOT / 'target' / 'host-tests'
    base.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix='run-', dir=base))
    start = time.monotonic()
    print(f'Running {len(selected)} host suites; logs: {output}', flush=True)
    results = []
    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        pending = {executor.submit(run_suite, name, available[name], output, args.rustc, args.timeout): name for name in selected}
        for task in as_completed(pending):
            try:
                result = task.result()
            except Exception as error:
                result = dict(suite=pending[task], status='failed', phase='runner', error=str(error))
            results.append(result)
            detail = f'{result["passed"]} tests' if result['status'] == 'passed' else result['phase'] + (' timeout' if result.get('timed_out') else ' failure')
            print(f'{result["status"].upper():6} {result["suite"]}: {detail}', flush=True)
    failed = [result for result in results if result['status'] != 'passed']
    report = dict(schema=1, compiler=compiler, python=sys.version, platform=sys.platform,
                  command=sys.argv, seconds=round(time.monotonic()-start, 3),
                  suites=sorted(results, key=lambda r: r['suite']),
                  passed=sum(r.get('passed', 0) for r in results), ignored=sum(r.get('ignored', 0) for r in results),
                  failed_tests=sum(r.get('failed', 0) for r in results), failed_suites=len(failed), status='failed' if failed else 'passed')
    summary = output / 'summary.json'
    summary.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(f'{len(results)-len(failed)}/{len(results)} suites passed; {report["passed"]} tests passed; {report["failed_tests"]} failed; {report["ignored"]} ignored. Report: {summary}', flush=True)
    for result in failed:
        print(f'Failure: {result["suite"]}: {result.get("test_log", result.get("compile_log", result.get("error")))}', flush=True)
    return 1 if failed else 0


if __name__ == '__main__':
    raise SystemExit(main())
