# Portions Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# mpatch.py - CFFI implementation of mpatch.c
#
# Copyright 2016 Maciej Fijalkowski <fijall@gmail.com>
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2 or any later version.

from __future__ import absolute_import

from ..pure.mpatch import *  # noqa: F401, F403

# pyre-fixme[21]: Could not find name `_mpatch` in `sapling.cffi`.
from . import _mpatch


# pyre-fixme[16]: Module `cffi` has no attribute `_mpatch`.
ffi = _mpatch.ffi
# pyre-fixme[16]: Module `cffi` has no attribute `_mpatch`.
lib = _mpatch.lib


@ffi.def_extern()
def cffi_get_next_item(arg, pos):
    all, bins = ffi.from_handle(arg)
    container = ffi.new("struct mpatch_flist*[1]")
    to_pass = ffi.new("char[]", str(bins[pos]))
    all.append(to_pass)
    r = lib.mpatch_decode(to_pass, len(to_pass) - 1, container)
    if r < 0:
        return ffi.NULL
    return container[0]


def patches(text, bins):
    lgt = len(bins)
    all = []
    if not lgt:
        return text
    arg = (all, bins)
    patch = lib.mpatch_fold(ffi.new_handle(arg), lib.cffi_get_next_item, 0, lgt)
    if not patch:
        raise mpatchError("cannot decode chunk")  # noqa: F405
    outlen = lib.mpatch_calcsize(len(text), patch)
    if outlen < 0:
        lib.mpatch_lfree(patch)
        raise mpatchError("inconsistency detected")  # noqa: F405
    buf = ffi.new("char[]", outlen)
    if lib.mpatch_apply(buf, text, len(text), patch) < 0:
        lib.mpatch_lfree(patch)
        raise mpatchError("error applying patches")  # noqa: F405
    res = ffi.buffer(buf, outlen)[:]
    lib.mpatch_lfree(patch)
    return res
