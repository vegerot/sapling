# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

setup configuration

  $ cat >> "$ACL_FILE" << ACLS
  > {
  >   "repos": {
  >     "orig": {
  >       "actions": {
  >         "read": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"],
  >         "write": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"],
  >         "bypass_readonly": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"]
  >       }
  >     },
  >     "backup": {
  >       "actions": {
  >         "read": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"],
  >         "write": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"],
  >          "bypass_readonly": ["$CLIENT0_ID_TYPE:$CLIENT0_ID_DATA"]
  >       }
  >     }
  >   }
  > }
  > ACLS
  $ REPOID=0 REPONAME=orig ACL_NAME=orig setup_common_config blob_files
  $ REPOID=1 READ_ONLY_REPO=1 REPONAME=backup ACL_NAME=backup setup_common_config blob_files
  $ export BACKUP_REPO_ID=1
  $ cd $TESTTMP

setup repo
  $ hginit_treemanifest repo
  $ cd repo

  $ echo s > smallfile
  $ hg commit -Aqm "add small file"
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > pushrebase =
  > EOF

  $ hg bookmark master_bookmark -r tip
  $ cd ..

Blobimport the hg repo to Mononoke
  $ REPOID=0 blobimport repo/.hg orig
  $ REPONAME=orig
  $ REPOID=1 blobimport repo/.hg backup

start mononoke
  $ start_and_wait_for_mononoke_server
Push to Mononoke
  $ hg clone -q mono:orig client-push --noupdate
  $ cd $TESTTMP/client-push
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > pushrebase =
  > EOF
  $ hg up -q tip

  $ mkcommit pushcommit
  $ hg push -r . --to master_bookmark -q

Sync it to another client should fail, because of readonly repo
  $ cd $TESTTMP
  $ mononoke_backup_sync backup sync-once 1 2>&1 | grep 'Repo is locked' | sed -e 's/^[ ]*//' | sort --unique
  * Repo is locked: Set by config option (glob)


Sync it to another client with bypass-readonly should success
  $ cd $TESTTMP
  $ mononoke_backup_sync backup sync-once 1 --bypass-readonly 2>&1 | grep 'successful sync'
  * successful sync of entries [2]* (glob)

Check synced commit in backup repo
  $ hg clone -q mono:backup backup --noupdate
  $ cd "$TESTTMP/backup"
  $ REPONAME=backup
  $ hg pull -q
  $ hg log -r master_bookmark -T '{node}\n'
  9fdce596be1b7052b777aa0bf7c5e87b00397a6f
