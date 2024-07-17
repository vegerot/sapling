#!/usr/bin/env python
from __future__ import absolute_import, print_function

import os

from sapling import commands, localrepo, ui as uimod


u = uimod.ui.load()

u.write("% creating repo\n")
localrepo.localrepository(u, "repo", create=True)
repo = localrepo.localrepository(u, "repo")
os.chdir("repo")

f = open("test.py", "w")
try:
    f.write("foo\n")
finally:
    f.close

u.write("% add and commit\n")
commands.add(u, repo, "test.py")
commands.commit(u, repo, message="*")
commands.status(u, repo, clean=True)


u.write("% change\n")
f = open("test.py", "w")
try:
    f.write("bar\n")
finally:
    f.close()

# this would return clean instead of changed before the fix
commands.status(u, repo, clean=True, modified=True)
