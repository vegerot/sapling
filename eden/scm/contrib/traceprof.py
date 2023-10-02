#!/usr/bin/env python
# Portions Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# Copyright Olivia Mackall <olivia@selenic.com> and others
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2 or any later version.

from __future__ import absolute_import, print_function

import os
import sys

from sapling import ui as uimod

from sapling.ext import traceprof


if __name__ == "__main__":
    sys.argv = sys.argv[1:]
    if not sys.argv:
        print("usage: traceprof.py <script> <arguments...>", file=sys.stderr)
        sys.exit(2)
    sys.path.insert(0, os.path.abspath(os.path.dirname(sys.argv[0])))
    u = uimod.ui()
    u.setconfig("traceprof", "timethreshold", 0)
    with traceprof.profile(u, sys.stderr):
        with open(sys.argv[0]) as f:
            exec(f.read())
