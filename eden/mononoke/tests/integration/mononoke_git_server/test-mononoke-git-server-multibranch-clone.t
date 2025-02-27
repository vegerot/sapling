# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

# Integration test based on scenario: https://internalfb.com/excalidraw/EX180257
  $ . "${TEST_FIXTURES}/library.sh"
  $ REPOTYPE="blob_files"
  $ setup_common_config $REPOTYPE
  $ GIT_REPO_ORIGIN="${TESTTMP}/origin/repo-git"
  $ GIT_REPO="${TESTTMP}/repo-git"

# Setup git repository
  $ mkdir -p "$GIT_REPO_ORIGIN"
  $ cd "$GIT_REPO_ORIGIN"
  $ git init -q
# Add a base commit on master_bookmark
  $ echo "this is origin" > origin
  $ git add origin
  $ git commit -qam "Add origin"
# Create branch R1 from master_bookmark and add three commits on it
  $ git checkout -qb R1
  $ echo "this is file1" > file1
  $ git add file1
  $ git commit -qam "Add file1 in branch R1"
  $ echo "this is file1.1" > file1
  $ git add file1
  $ git commit -qam "Modified file1 -> file1.1 in branch R1"
  $ echo "this is file1.2" > file1
  $ git add file1
  $ git commit -qam "Modified file1.1 -> file1.2 in branch R1"
# Create branch R2 from master_bookmark and add two commits on it
  $ git checkout -qb R2 master_bookmark
  $ echo "this is file1.2" > file1
  $ git add file1
  $ git commit -qam "Add file1.2 in branch R2"
  $ echo "this is file1" > file1
  $ git add file1
  $ git commit -qam "Modified file1.2 -> file1 in branch R2"

  $ cd "$TESTTMP"
  $ git clone --mirror "$GIT_REPO_ORIGIN" repo-git
  Cloning into bare repository 'repo-git'...
  done.

# Capture all the known Git objects from the repo
  $ cd $GIT_REPO
  $ git rev-list --objects --all | git cat-file --batch-check='%(objectname) %(objecttype) %(rest)' | sort > $TESTTMP/object_list

# Import it into Mononoke
  $ cd "$TESTTMP"
  $ quiet gitimport "$GIT_REPO" --derive-hg --generate-bookmarks full-repo

# Start up the Mononoke Git Service
  $ mononoke_git_service
# Clone the Git repo from Mononoke
  $ quiet git_client clone $MONONOKE_GIT_SERVICE_BASE_URL/$REPONAME.git
# Verify that we get the same Git repo back that we started with
  $ cd $REPONAME  
  $ git rev-list --objects --all | git cat-file --batch-check='%(objectname) %(objecttype) %(rest)' | sort > $TESTTMP/new_object_list
  $ diff -w $TESTTMP/new_object_list $TESTTMP/object_list  
