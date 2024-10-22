# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"
  $ REPOTYPE="blob_files"
  $ setup_common_config $REPOTYPE
  $ GIT_REPO_ORIGIN="${TESTTMP}/origin/repo-git"
  $ GIT_REPO="${TESTTMP}/repo-git"
  $ HG_REPO="${TESTTMP}/repo"
  $ BUNDLE_PATH="${TESTTMP}/repo_bundle.bundle"

# Setup git repository
  $ mkdir -p "$GIT_REPO_ORIGIN"
  $ cd "$GIT_REPO_ORIGIN"
  $ git init -q

# Add FileA with some content and commit it
  $ echo "This is a file that will be later deleted in a future commit" > FileA
  $ git add FileA
  $ git commit -q -am "Add FileA"
  $ git tag -a -m "First commit tag" first_tag

# Add FileB with different content and commit it
  $ echo "This is a file whose content will change in a future commit" > FileB
  $ git add FileB
  $ git commit -q -am "Add FileB"
  $ git tag -a empty_tag -m ""

# Create a file with exact same contents as an earlier file but in a different directory
  $ mkdir -p src/path/to
  $ echo "This is a file that will be later deleted in a future commit" > src/path/to/FileA
  $ git add .
  $ git commit -q -am "Add src/path/to/FileB"

# Delete an existing file from the repository
  $ rm -rf FileA
  $ git add .
  $ git commit -q -am "Removed FileA from repo"

# Change the content of FileB
  $ echo "Changed FileB content" > FileB
  $ git add .
  $ git commit -q -am "Changed FileB"
  $ git tag -a changed_tag -m "Tag for change in FileB"

# Re-add the same FileA with the same content as before and return the content
# of FileB to be the same content as before
  $ echo "This is a file that will be later deleted in a future commit" > FileA
  $ echo "This is a file whose content will change in a future commit" > FileB
  $ git add .
  $ git commit -q -am "Brought FileA and FileB back to their original state"

# Clone the Git repo  
  $ cd "$TESTTMP"
  $ git clone --mirror "$GIT_REPO_ORIGIN" repo-git
  Cloning into bare repository 'repo-git'...
  done.

# Capture all the known Git objects from the repo
  $ cd $GIT_REPO
  $ git rev-list --objects --all | git cat-file --batch-check='%(objectname) %(objecttype) %(rest)' | sort > $TESTTMP/object_list

# Get the repository log
  $ git log --pretty=format:"%h %an %s %D" > $TESTTMP/repo_log

# Import it into Mononoke
  $ cd "$TESTTMP"
  $ with_stripped_logs gitimport "$GIT_REPO" --derive-hg --generate-bookmarks full-repo
  using repo "repo" repoid RepositoryId(0)
  GitRepo:*repo-git commit 6 of 6 - Oid:* => Bid:* (glob)
  Hg: Sha1(*): HgManifestId(HgNodeHash(Sha1(*))) (glob)
  Hg: Sha1(*): HgManifestId(HgNodeHash(Sha1(*))) (glob)
  Hg: Sha1(*): HgManifestId(HgNodeHash(Sha1(*))) (glob)
  Hg: Sha1(*): HgManifestId(HgNodeHash(Sha1(*))) (glob)
  Hg: Sha1(*): HgManifestId(HgNodeHash(Sha1(*))) (glob)
  Hg: Sha1(*): HgManifestId(HgNodeHash(Sha1(*))) (glob)
  Ref: "refs/heads/master_bookmark": Some(ChangesetId(Blake2(*))) (glob)
  Ref: "refs/tags/changed_tag": Some(ChangesetId(Blake2(*))) (glob)
  Ref: "refs/tags/empty_tag": Some(ChangesetId(Blake2(*))) (glob)
  Ref: "refs/tags/first_tag": Some(ChangesetId(Blake2(*))) (glob)
  Initializing repo: repo
  Initialized repo: repo
  All repos initialized. It took: * seconds (glob)
  Bookmark: "heads/master_bookmark": ChangesetId(Blake2(*)) (created) (glob)
  Bookmark: "tags/changed_tag": ChangesetId(Blake2(*)) (created) (glob)
  Bookmark: "tags/empty_tag": ChangesetId(Blake2(*)) (created) (glob)
  Bookmark: "tags/first_tag": ChangesetId(Blake2(*)) (created) (glob)

# Regenerate the Git repo out of the Mononoke repo
  $ mononoke_newadmin git-bundle create from-repo -R repo --output-location "$BUNDLE_PATH"
# Ensure that Git considers this a valid bundle
  $ cd $GIT_REPO
  $ git bundle verify -q $BUNDLE_PATH
  $TESTTMP/repo_bundle.bundle is okay

# Create a new empty folder for containing the repo
  $ mkdir $TESTTMP/git_client_repo  
  $ cd "$TESTTMP"
  $ git clone --mirror "$BUNDLE_PATH" git_client_repo
  Cloning into bare repository 'git_client_repo'...
  $ cd git_client_repo

# Get the repository log and verify if its the same as earlier
  $ git log --pretty=format:"%h %an %s %D" > $TESTTMP/new_repo_log
  $ diff -w $TESTTMP/new_repo_log $TESTTMP/repo_log

# Dump all the known Git objects into a file
  $ git rev-list --objects --all | git cat-file --batch-check='%(objectname) %(objecttype) %(rest)' | sort > $TESTTMP/new_object_list

# Ensure that there are no differences between the set of objects by diffing both object list files
  $ diff -w $TESTTMP/new_object_list $TESTTMP/object_list   
