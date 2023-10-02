# Portions Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# Copyright 2016 Mercurial Contributors
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2 or any later version.

from __future__ import absolute_import

import os

import cffi


ffi = cffi.FFI()
with open(os.path.join(os.path.join(os.path.dirname(__file__), ".."), "bdiff.c")) as f:
    ffi.set_source("cffi._bdiff", f.read(), include_dirs=["."])
ffi.cdef(
    """
struct bdiff_line {
    int hash, n, e;
    ssize_t len;
    const char *l;
};

struct bdiff_hunk;
struct bdiff_hunk {
    int a1, a2, b1, b2;
    struct bdiff_hunk *next;
};

int bdiff_splitlines(const char *a, ssize_t len, struct bdiff_line **lr);
int bdiff_diff(struct bdiff_line *a, int an, struct bdiff_line *b, int bn,
    struct bdiff_hunk *base);
void bdiff_freehunks(struct bdiff_hunk *l);
void free(void*);
"""
)

if __name__ == "__main__":
    ffi.compile()
