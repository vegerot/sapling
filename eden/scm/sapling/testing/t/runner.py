# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

import collections
import doctest
import multiprocessing
import os
import re
import sys
import textwrap
import threading
import traceback
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, List, Optional, Union

from . import hghave
from .runtime import hasfeature, Mismatch
from .transform import transform


@dataclass
class TestId:
    """information about a test identity"""

    name: str
    path: str

    @classmethod
    def frompath(cls, path: str):
        if path.startswith("doctest:"):
            name = path
            modname = name[8:]
            __import__(modname)
            # pyre-fixme[9]: path has type `str`; used as `Optional[str]`.
            path = sys.modules[modname].__file__
            return cls(name=name, path=path)

        path = os.path.abspath(path)
        if path.endswith(".py"):
            # try to convert the .py path to doctest:module
            modnames = sorted(n for n in sys.modules if "." not in n)
            for name in modnames:
                mod = sys.modules[name]
                modpath = getattr(mod, "__file__", None)
                if not modpath or os.path.basename(modpath) != "__init__.py":
                    continue
                prefix = os.path.dirname(modpath) + os.path.sep
                if not path.startswith(prefix):
                    continue
                relpath = path[len(prefix) :].replace("\\", "/")
                for suffix in ["/__init__.py", ".py"]:
                    if relpath.endswith(suffix):
                        relpath = relpath[: -len(suffix)]
                        break
                modname = f"{mod.__name__}.{relpath.replace('/', '.')}"
                return cls.frompath(f"doctest:{modname}")
            # try harder, using sys.path
            # This is needed when the modules being used are static:*.
            for root in sys.path:
                relpath = os.path.relpath(path, root)
                if not relpath.startswith(".."):
                    if relpath.endswith("__init__.py"):
                        # strip "/__init__.py"
                        name = os.path.dirname(relpath)
                    else:
                        # strip ".py"
                        name = relpath[:-3]
                    modname = name.replace("\\", ".").replace("/", ".")
                    if modname in sys.modules:
                        # double check that the source code matches
                        mod = sys.modules[modname]

                        if mod.__file__.startswith("static:"):
                            import inspect

                            source1 = inspect.getsource(mod)
                            with open(path, "rb") as f:
                                source2 = f.read().decode()

                            if source1 != source2:
                                sys.stderr.write(
                                    f"warning: doctest is using an older static version of module {modname} that no longer matches on-disk {path}\n"
                                )
                                sys.stderr.flush()

                        return cls.frompath(f"doctest:{modname}")
            raise RuntimeError(
                f"cannot find Python module name for {path=} to run doctest\n"
                "hint: try 'doctest:module.name' instead of file path\n"
            )
        else:
            name = os.path.basename(path)
            return cls(name=name, path=path)

    @property
    def modname(self) -> Optional[str]:
        if self.name.startswith("doctest:"):
            return self.name.split(":", 1)[1]
        return None


@dataclass
class TestResult:
    testid: TestId
    exc: Optional[Exception] = None
    tb: Optional[str] = None


class MismatchError(AssertionError):
    pass


class TestRunner:
    """runner for .t tests

    The runner reports test progress in a streaming fashion and does
    not write to stdout or stderr.

    With context is used for explicit resource cleanup:

        mismatches = []
        with TestRunner(paths) as runner:
            for item in runner:
                if isinstance(item, Mismatch):
                    # print mismatch
                    mismatches.append(item)
                    ...
                else:
                    assert isinstance(item, TestResult)
                    # do something with TestResult
        if autofix:
            fixmismatches(mismatches)
    """

    def __init__(
        self, paths: List[str], jobs: int = 1, exts=List[str], isolate: bool = True
    ):
        self.testids = [TestId.frompath(p) for p in paths]
        self.jobs = jobs or multiprocessing.cpu_count()
        self.exts = exts
        self.isolate = isolate

    def __iter__(self):
        return self

    def __next__(self) -> Union[TestResult, Mismatch]:
        """obtain next test result, or mismatch block"""
        try:
            # pyre-fixme[16]: `TestRunner` has no attribute `resultqueue`.
            v = self.resultqueue.get()
            if v is StopIteration:
                raise StopIteration
            return v
        except ValueError:
            raise StopIteration

    def __enter__(self):
        """prepare the test runner environment, namely a way to receive test results"""
        # use 'spawn' instead of 'fork' to clean up Python global state.
        # Python global state might be polluted by uisetup or tests.
        self.mp = mp = multiprocessing.get_context("spawn")
        self.resultqueue = mp.Queue()
        self.sem = mp.Semaphore(self.jobs)
        self.running = True
        if self.isolate:
            # start running tests in background
            self.runnerthread = thread = threading.Thread(
                target=self._start, daemon=True
            )
            thread.start()
        else:
            self._start()
        return self

    def __exit__(self, et, ev, tb):
        """stop and clean up test environment"""
        self.running = False
        if self.isolate:
            self.runnerthread.join()

    def _start(self):
        """run tests (blocking, intended to run in thread)"""
        processes = []
        try:
            for t in self.testids:
                kwargs = {
                    "testid": t,
                    "sem": self.isolate and self.sem or None,
                    "resultqueue": self.resultqueue,
                    "exts": self.exts,
                }
                if self.isolate:
                    # limit concurrency, release by the Process
                    while True:
                        acquired = self.sem.acquire(timeout=1)
                        if not self.running:
                            return
                        if acquired:
                            break
                    p = self.mp.Process(
                        target=_spawnmain,
                        kwargs=kwargs,
                    )
                    p.start()
                    processes.append(p)
                else:
                    _spawnmain(**kwargs)
        finally:
            for p in processes:
                p.join()
            self.resultqueue.put(StopIteration)


def _spawnmain(
    testid: TestId,
    exts: List[str],
    # pyre-fixme[11]: Annotation `Semaphore` is not defined as a type.
    sem: Optional[multiprocessing.Semaphore],
    resultqueue: multiprocessing.Queue,
):
    """run a test and report progress back
    intended to be spawned via multiprocessing.Process.
    """

    hasmismatch = False

    def mismatchcb(mismatch: Mismatch):
        nonlocal hasmismatch
        hasmismatch = True
        mismatch.testname = testid.name
        resultqueue.put(mismatch)

    result = TestResult(testid=testid)
    try:
        runtest(testid, exts, mismatchcb)
    except TestNotFoundError as e:
        result.exc = e
    except Exception as e:
        result.exc = e
        result.tb = traceback.format_exc(limit=-1)
    finally:
        if result.exc is None and hasmismatch:
            result.exc = MismatchError("output mismatch")
        resultqueue.put(result)
        if sem:
            resultqueue.close()
            sem.release()


class TestNotFoundError(FileNotFoundError):
    def __str__(self):
        return "not found"


def runtest(testid: TestId, exts: List[str], mismatchcb: Callable[[Mismatch], None]):
    """run a .t test at the given path

    The generated Python code is written at __pycache__/ttest/<test>.py.
    Return output mismatches.
    """
    if testid.modname:
        return rundoctest(testid, mismatchcb)
    else:
        return runttest(testid, exts, mismatchcb)


class doctestrunner(doctest.DocTestRunner):
    """doctest runner that reports output mismatches as Mismatch"""

    def __init__(self, testname: str, mismatchcb: Callable[[Mismatch], None]):
        optionflags = doctest.IGNORE_EXCEPTION_DETAIL
        super().__init__(verbose=False, optionflags=optionflags)
        self.testname = testname
        self.mismatchcb = mismatchcb

    def report_failure(
        self, out, test: doctest.DocTest, example: doctest.Example, got: str
    ):
        # see doctest.OutputChecker.output_difference
        if not (self.optionflags & doctest.DONT_ACCEPT_BLANKLINE):
            got = re.sub("(?m)^[ ]*(?=\n)", doctest.BLANKLINE_MARKER, got)

        # pyre-fixme[58]: `+` is not supported for operand types `Optional[int]` and
        #  `int`.
        srcloc = test.lineno + example.lineno
        outloc = srcloc + example.source.count("\n")
        endloc = outloc + example.want.count("\n")
        src = ">>> " + textwrap.indent(example.source, "... ")[4:]
        mismatch = Mismatch(
            actual=got,
            expected=example.want,
            src=src,
            srcloc=srcloc,
            outloc=outloc,
            endloc=endloc,
            indent=example.indent,
            # pyre-fixme[6]: For 8th param expected `str` but got `Optional[str]`.
            filename=test.filename,
            testname=self.testname,
        )
        self.mismatchcb(mismatch)

    def report_unexpected_exception(self, out, test, example, excinfo):
        exctype, excvalue, exctb = excinfo
        excmsg = str(excvalue)
        exctypestr = exctype.__name__
        if excmsg:
            excstr = f"{exctypestr}: {excmsg}"
        else:
            excstr = exctypestr
        got = f"Traceback (most recent call last):\n  ...\n{excstr}\n"
        return self.report_failure(out, test, example, got)


def rundoctest(testid: TestId, mismatchcb: Callable[[Mismatch], None]):
    """run doctest for the given module, report Mismatch via mismatchcb"""
    modname = testid.modname
    # pyre-fixme[6]: For 1st param expected `str` but got `Optional[str]`.
    __import__(modname)
    # pyre-fixme[6]: For 1st param expected `str` but got `Optional[str]`.
    mod = sys.modules[modname]
    finder = doctest.DocTestFinder()
    runner = doctestrunner(testid.name, mismatchcb)
    for test in finder.find(mod):
        runner.run(test)


def runttest(testid: TestId, exts: List[str], mismatchcb: Callable[[Mismatch], None]):
    path = Path(testid.path)
    testdir = path.parent
    exts = exts[:]
    try:
        tcode = path.read_bytes().decode()
    except FileNotFoundError as e:
        raise TestNotFoundError(e)

    exeneeded = set()

    # For running hg from within Buck, we have a Python wrapper for making it
    # compatible with both POSIX and Windows. This wrapper needs fbpython
    # for being run, so we add it to the list of needed exes since debugruntest
    # modifies the PATH env var
    if "BUCK_DEFAULT_RUNTIME_RESOURCES" in os.environ:
        exeneeded.add("fbpython")

    def hasfeaturetracked(feature):
        if feature in {"ext.parallel"}:
            exts.append("sapling.testing.ext.parallel")
            return True
        result = hasfeature(feature)
        if result and feature in hghave.exes:
            exeneeded.add(feature)
        return result

    testcases = []

    body = transform(
        tcode,
        indent=8,
        filename=str(path),
        hasfeature=hasfeaturetracked,
        registertestcase=testcases.append,
    )

    testcases = [f"_run_once(testcase='{tc}')\n" for tc in testcases]
    if not testcases:
        testcases.append("_run_once()\n")

    extcode = []
    for ext in exts:
        extcode.append(
            f"""
__import__({repr(ext)})
sys.modules[{repr(ext)}].testsetup(t)
"""
        )

    extcode = textwrap.indent("".join(extcode).strip(), "    ")

    pycode = f"""
import sys
from sapling.testing.t.runtime import TestTmp

TESTFILE = {repr(str(path))}
TESTDIR = {repr(str(testdir))}

def _run_once(testcase=None):
    t = TestTmp(tmpprefix={repr(path.name)}, testcase=testcase)
    t.setenv("TESTFILE", TESTFILE)
    t.setenv("TESTDIR", TESTDIR)
    t.setenv("RUNTESTDIR", TESTDIR)  # compatibility: path of run-tests.py

    for exe in {sorted(exeneeded)}:
        t.requireexe(exe)

    sys.path += [TESTDIR, str(t.path)]

{extcode}

    with t:
        _pristine_globals = dict(globals())
        globals().update(t.pyenv)

{body}

        # Restore globals since pydoceval locals are persisted as
        # global variables (and we don't want variables crossing test
        # cases).
        globals().clear()
        globals().update(_pristine_globals)

{"".join(testcases)}
"""

    pypath = (path.parent / "__pycache__" / "ttest" / path.name).with_suffix(".py")

    # write it down for easier investigation
    pypath.parent.mkdir(parents=True, exist_ok=True)
    pypath.write_bytes(pycode.encode())

    compiled = compile(pycode, str(pypath), "exec")
    pyenv = {"mismatchcb": mismatchcb}
    exec(compiled, pyenv)


class filelinesdict(collections.defaultdict):
    """{path: [line]} dict - read path on demand"""

    def __missing__(self, key: str) -> List[str]:
        with open(key, "rb") as f:
            lines = f.read().decode().splitlines(True)
            self[key] = lines
        return lines


def fixmismatches(mismatches: List[Mismatch]):
    """update test files to fix mismatches"""
    mismatches = sorted(mismatches, key=lambda m: (m.filename, -m.outloc))
    filelines = filelinesdict()
    lastline = collections.defaultdict(lambda: sys.maxsize)
    for m in mismatches:
        if lastline[m.filename] < m.outloc:
            # already changed or fixed (ex. conflicted fix in a loop)
            continue
        lastline[m.filename] = m.outloc
        lines = filelines[m.filename]
        # TODO: try to preserve (glob)s.
        # 'lambda l: True' ensures blank lines are indented too
        lines[m.outloc : m.endloc] = textwrap.indent(
            m.actual, " " * m.indent, lambda l: True
        )
    for path, lines in filelines.items():
        with open(path, "wb") as f:
            f.write("".join(lines).encode())
