/*
 * Portions Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

/*
 Copyright 2005, 2006 Olivia Mackall <olivia@selenic.com>

 This software may be used and distributed according to the terms
 of the GNU General Public License, incorporated herein by reference.
*/
#ifndef _HG_MPATCH_H_
#define _HG_MPATCH_H_

#define MPATCH_ERR_NO_MEM -3
#define MPATCH_ERR_CANNOT_BE_DECODED -2
#define MPATCH_ERR_INVALID_PATCH -1
#include "eden/scm/sapling/compat.h"

struct mpatch_frag {
  int start, end, len;
  const char* data;
};

struct mpatch_flist {
  struct mpatch_frag *base, *head, *tail;
};

int mpatch_decode(const char* bin, ssize_t len, struct mpatch_flist** res);
ssize_t mpatch_calcsize(ssize_t len, struct mpatch_flist* l);
void mpatch_lfree(struct mpatch_flist* a);
int mpatch_apply(
    char* buf,
    const char* orig,
    ssize_t len,
    struct mpatch_flist* l);
struct mpatch_flist* mpatch_fold(
    void* bins,
    struct mpatch_flist* (*get_next_item)(void*, ssize_t),
    ssize_t start,
    ssize_t end);

#endif
