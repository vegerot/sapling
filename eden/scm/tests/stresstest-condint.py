#!/usr/bin/env python
# Portions Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# Copyright 2006, 2007 Olivia Mackall <olivia@selenic.com>
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2 or any later version.
from __future__ import absolute_import

import ctypes
import os
import random
import threading
import time


def importrustthreading():
    try:
        import bindings
    except ImportError:
        print("Cannot import bindings to test Rust Condition.")
        print("Did you forget to run the test using 'sl/hg debugpython'?")
        print("Set COND=py to test the CPython's Condition implementation.")
        raise

    return bindings.threading


try:
    xrange
except NameError:
    xrange = range


class ThreadInterrupt(RuntimeError):
    pass


def interrupt(thread, exc=ThreadInterrupt):
    if thread.is_alive():
        ctypes.pythonapi.PyThreadState_SetAsyncExc(
            ctypes.c_long(thread.ident), ctypes.py_object(exc)
        )


stop = False


def lockloop(cond, workaround):
    try:
        while True:
            try:
                # There are many ways for the pure Python RLock / Condition
                # implementation to go wrong. One example RLock._release_save
                # (used by Condition.wait, in Python 2):
                #
                #     def _release_save(self):
                #         count = self.__count
                #         self.__count = 0
                #         owner = self.__owner
                #         self.__owner = None
                #         # (Interrupt here will cause owner reset to None
                #         #  without unlocking. Therefore no thread thinks
                #         #  it owns the lock and they simply deadlock)
                #         self.__block.release()
                if workaround:
                    b = importrustthreading().bug29988wrapper(cond)
                else:
                    b = cond
                with b, b, b, b:
                    # Some busy loops to make Python more easily to do context
                    # switches.
                    count = 0
                    n = 10 ** random.randint(1, 5)
                    for i in xrange(n):
                        count += i
                    timeout = random.randint(-10, 20)
                    if timeout < 0:
                        cond.wait()
                    else:
                        cond.wait(timeout * 0.01)
                    for i in xrange(n):
                        count -= i
                    assert count == 0
            except ThreadInterrupt:
                pass
            except SystemExit:
                return
            finally:
                owned = cond._is_owned()
                if owned:
                    # At the time of writing, Python can skip __exit__ and
                    # there is no way to fix that from native code. This
                    # means no matter how RLock is implemented, the lock might
                    # still be held. See https://bugs.python.org/issue29988
                    msg = "%r should not be owned by this thread" % cond
                    cond.release()
                    global stop
                    if not stop:
                        stop = True
                        assert not owned, msg
    except ThreadInterrupt:
        # RuntimeError can also be in the finally block above. Silence it.
        pass


def mainloop(cond, workaround):
    threads = []
    count = 1000
    maxthread = 10
    try:
        for i in xrange(count + 1):
            global stop
            if stop:
                break
            while len(threads) < maxthread:
                t = threading.Thread(target=lockloop, args=(cond, workaround))
                t.start()
                threads.append(t)
            # Give new threads chance to take the lock
            time.sleep(0.001)
            with cond:
                cond.notify_all()
            for t in random.sample(threads, 2):
                interrupt(t)
            for t in threads:
                if not t.is_alive():
                    t.join()
            threads = [t for t in threads if t.is_alive()]
            if i % 100 == 0:
                print("%4d / %d" % (i, count))
    finally:
        print("\nCleaning up")
        for t in threads:
            interrupt(t, SystemExit)
        with cond:
            cond.notify_all()
        for t in threads:
            t.join()
        if stop:
            print("Failed")
        else:
            print("Passed this time. Does not mean bug-free, though.")


if __name__ == "__main__":
    print(
        (
            "Run 'kill -9 %s' from another terminal if this gets stuck.\n\n"
            "Passing != bug-free. AssertionError, RuntimeError or hanging = buggy\n\n"
            "Affected by https://bugs.python.org/issue29988, this test is expected\n"
            "to fail with all Condition implementation if run enough times.\n"
            "\n"
            "At the time of writing, CPython 3.8 is known to fail the test.\n"
            "The Rust Condition implementation seems okay with CPython 3.8.\n"
            "Technically, with W=1 (Issue29988 workaround), it is even less\n"
            "likely to fail.\n"
        )
        % os.getpid()
    )

    if os.environ.get("COND") == "py":
        print("Using Python stdlib threading.Condition\n")
        cond = threading.Condition()
        workaround = False
    else:
        print("Using Rust threading.Condition")
        print("Rerun with 'COND=py' to test Python native threading.Condition\n")

        cond = importrustthreading().Condition()

        workaround = os.environ.get("W") == "1"
        if workaround:
            print("Issue29988 workaround is in effect\n")
        else:
            print("Rerun with 'W=1' to apply the issue29988 workaround\n")

    mainloop(cond, workaround)
