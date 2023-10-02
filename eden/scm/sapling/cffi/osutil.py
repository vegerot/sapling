# Portions Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# osutil.py - CFFI version of osutil.c
#
# Copyright 2016 Maciej Fijalkowski <fijall@gmail.com>
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2 or any later version.

from __future__ import absolute_import

import os
import stat as statmod

from .. import pycompat
from ..pure.osutil import *  # noqa: F401, F403
from typing import List


if pycompat.isdarwin:
    # pyre-fixme[21]: Could not find name `_osutil` in `sapling.cffi`.
    from . import _osutil

    # pyre-fixme[16]: Module `cffi` has no attribute `_osutil`.
    ffi = _osutil.ffi
    # pyre-fixme[16]: Module `cffi` has no attribute `_osutil`.
    lib = _osutil.lib

    listdir_batch_size = 4096
    # tweakable number, only affects performance, which chunks
    # of bytes do we get back from getattrlistbulk

    attrkinds: List[None] = [
        None
    ] * 20  # we need the max no for enum VXXX, 20 is plenty

    # pyre-fixme[6]: For 2nd param expected `None` but got `int`.
    attrkinds[lib.VREG] = statmod.S_IFREG
    # pyre-fixme[6]: For 2nd param expected `None` but got `int`.
    attrkinds[lib.VDIR] = statmod.S_IFDIR
    # pyre-fixme[6]: For 2nd param expected `None` but got `int`.
    attrkinds[lib.VLNK] = statmod.S_IFLNK
    # pyre-fixme[6]: For 2nd param expected `None` but got `int`.
    attrkinds[lib.VBLK] = statmod.S_IFBLK
    # pyre-fixme[6]: For 2nd param expected `None` but got `int`.
    attrkinds[lib.VCHR] = statmod.S_IFCHR
    # pyre-fixme[6]: For 2nd param expected `None` but got `int`.
    attrkinds[lib.VFIFO] = statmod.S_IFIFO
    # pyre-fixme[6]: For 2nd param expected `None` but got `int`.
    attrkinds[lib.VSOCK] = statmod.S_IFSOCK

    class stat_res:
        def __init__(self, st_mode, st_mtime, st_size):
            self.st_mode = st_mode
            self.st_mtime = st_mtime
            self.st_size = st_size

    tv_sec_ofs = ffi.offsetof("struct timespec", "tv_sec")
    buf = ffi.new("char[]", listdir_batch_size)

    def listdirinternal(dfd, req, stat, skip):
        ret = []
        while True:
            r = lib.getattrlistbulk(dfd, req, buf, listdir_batch_size, 0)
            if r == 0:
                break
            if r == -1:
                raise OSError(ffi.errno, os.strerror(ffi.errno))
            cur = ffi.cast("val_attrs_t*", buf)
            for i in range(r):
                lgt = cur.length
                assert lgt == ffi.cast("uint32_t*", cur)[0]
                ofs = cur.name_info.attr_dataoffset
                str_lgt = cur.name_info.attr_length
                base_ofs = ffi.offsetof("val_attrs_t", "name_info")
                name = str(
                    ffi.buffer(ffi.cast("char*", cur) + base_ofs + ofs, str_lgt - 1)
                )
                tp = attrkinds[cur.obj_type]
                if name == "." or name == "..":
                    continue
                if skip == name and tp == statmod.S_ISDIR:
                    return []
                if stat:
                    mtime = cur.mtime.tv_sec
                    mode = (cur.accessmask & ~lib.S_IFMT) | tp
                    ret.append(
                        (
                            name,
                            tp,
                            stat_res(
                                st_mode=mode, st_mtime=mtime, st_size=cur.datalength
                            ),
                        )
                    )
                else:
                    ret.append((name, tp))
                cur = ffi.cast("val_attrs_t*", int(ffi.cast("intptr_t", cur)) + lgt)
        return ret

    def listdir(path, stat: bool = False, skip=None):
        req = ffi.new("struct attrlist*")
        req.bitmapcount = lib.ATTR_BIT_MAP_COUNT
        req.commonattr = (
            lib.ATTR_CMN_RETURNED_ATTRS
            | lib.ATTR_CMN_NAME
            | lib.ATTR_CMN_OBJTYPE
            | lib.ATTR_CMN_ACCESSMASK
            | lib.ATTR_CMN_MODTIME
        )
        req.fileattr = lib.ATTR_FILE_DATALENGTH
        dfd = lib.open(path, lib.O_RDONLY, 0)
        if dfd == -1:
            raise OSError(ffi.errno, os.strerror(ffi.errno))

        try:
            ret = listdirinternal(dfd, req, stat, skip)
        finally:
            try:
                lib.close(dfd)
            except BaseException:
                pass  # we ignore all the errors from closing, not
                # much we can do about that
        return ret
