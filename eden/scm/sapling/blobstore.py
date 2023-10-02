# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# blobstore.py - local blob storage

from __future__ import absolute_import

import hashlib

from . import error, util
from .i18n import _


class localblobstore:
    """A local blobstore.

    This blobstore is used both as a cache and as a staging area for large blobs
    to be uploaded to the remote blobstore.
    """

    def __init__(self, vfs, cachevfs):
        self.vfs = vfs
        self.cachevfs = cachevfs

    def write(self, oid, data):
        """Write blob to local blobstore."""
        contentsha256 = hashlib.sha256(data).hexdigest()
        if contentsha256 != oid:
            raise error.Abort(
                _("blobstore: sha256 mismatch (oid: %s, content: %s)")
                % (oid, contentsha256)
            )
        with self.vfs(oid, "wb", atomictemp=True) as fp:
            fp.write(data)

        # XXX: should we verify the content of the cache, and hardlink back to
        # the local store on success, but truncate, write and link on failure?
        if self.cachevfs and not self.cachevfs.exists(oid):
            self.vfs.linktovfs(oid, self.cachevfs)

    def read(self, oid):
        """Read blob from local blobstore."""
        if self.cachevfs and not self.vfs.exists(oid):
            self.cachevfs.linktovfs(oid, self.vfs)
        return self.vfs.read(oid)

    def has(self, oid):
        """Returns True if the local blobstore contains the requested blob,
        False otherwise."""
        return (self.cachevfs and self.cachevfs.exists(oid)) or self.vfs.exists(oid)

    def remove(self, oid):
        self.vfs.tryunlink(oid)

    def list(self):
        """Return a list of oids stored in this blobstore"""
        oids = []
        for entry in self.vfs.walk():
            oids += entry[-1]
        return sorted(oids)


class memlocal:
    """In-memory local blobstore for ad-hoc uploading/downloading without
    writing to the filesystem.

    Used by LFS (debuglfssingleupload and debuglfssingledownload).
    """

    def __init__(self):
        self._files = {}

    def write(self, oid, data):
        self._files[oid] = data

    def read(self, oid):
        return self._files[oid]

    def has(self, oid):
        return oid in self._files

    def vfs(self, oid, mode="r"):
        """wrapper for a "streaming" way to access a file

        Used by _gitlfsremote._basictransfer.
        """
        assert mode == "r"
        return util.stringio(self.read(oid))

    def remove(self, oid):
        if oid in self._files:
            del self._files[oid]

    def list(self):
        """Return a list of oids stored in this blobstore"""
        return list(sorted(self._files.keys()))


class unionstore:
    """A store which offers uniform access to in-memory store and local on-disk store."""

    def __init__(self, diskstore, memstore):
        self.diskstore = diskstore
        self.memstore = memstore

    def write(self, oid, data):
        self.diskstore.write(oid, data)

    def read(self, oid):
        if self.memstore.has(oid):
            return self.memstore.read(oid)
        else:
            return self.diskstore.read(oid)

    def has(self, oid):
        return self.memstore.has(oid) or self.diskstore.has(oid)

    def remove(self, oid):
        self.diskstore.remove(oid)
        self.memstore.remove(oid)

    def list(self):
        return list(sorted(set(self.diskstore.list() + self.memstore.list())))
