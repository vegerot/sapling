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
  $ echo "this is file1" > file1
  $ git add file1
  $ git commit -qam "Add file1"
  $ git tag -a -m "new tag" first_tag
# Add a recursive tag pointing to first_tag
  $ git config advice.nestedTag false
  $ git tag -a recursive_tag -m "recursive tag pointing to first_tag" $(git rev-parse first_tag)
# Add recursive tag pointing to another recursive tag
  $ git tag -a another_recursive_tag -m "recursive tag pointing to another recursive tag" $(git rev-parse recursive_tag)
  $ echo "this is file2" > file2
  $ git add file2
  $ git commit -qam "Add file2"
  $ git tag -a empty_tag -m ""
  $ cd "$TESTTMP"
  $ git clone --mirror "$GIT_REPO_ORIGIN" repo-git
  Cloning into bare repository 'repo-git'...
  done.
# Capture all the known Git objects from the repo
  $ cd $GIT_REPO
  $ git rev-list --objects --all | git cat-file --batch-check='%(objectname) %(objecttype) %(rest)' | sort > $TESTTMP/object_list


# Import it into Mononoke
  $ cd "$TESTTMP"
  $ with_stripped_logs gitimport "$GIT_REPO" --derive-hg --generate-bookmarks full-repo
  using repo "repo" repoid RepositoryId(0)
  GitRepo:*repo-git commit 2 of 2 - Oid:* => Bid:* (glob)
  Hg: Sha1(8ce3eae44760b500bf3f2c3922a95dcd3c908e9e): HgManifestId(HgNodeHash(Sha1(009adbc8d457927d2e1883c08b0692bc45089839)))
  Hg: Sha1(e8615d6f149b876be0a2f30a1c5bf0c42bf8e136): HgManifestId(HgNodeHash(Sha1(d92f8d2d10e61e62f65acf25cdd638ea214f267f)))
  Ref: "refs/heads/master_bookmark": Some(ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)))
  Ref: "refs/tags/another_recursive_tag": Some(ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)))
  Ref: "refs/tags/empty_tag": Some(ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)))
  Ref: "refs/tags/first_tag": Some(ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)))
  Ref: "refs/tags/recursive_tag": Some(ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)))
  Initializing repo: repo
  Initialized repo: repo
  All repos initialized. It took: * seconds (glob)
  Bookmark: "heads/master_bookmark": ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)) (created)
  Bookmark: "tags/another_recursive_tag": ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)) (created)
  Bookmark: "tags/empty_tag": ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)) (created)
  Bookmark: "tags/first_tag": ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)) (created)
  Bookmark: "tags/recursive_tag": ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)) (created)

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
  $ git log --pretty=format:"%h %an %s %D"
  e8615d6 mononoke Add file2 HEAD -> master_bookmark, tag: empty_tag
  8ce3eae mononoke Add file1 tag: recursive_tag, tag: first_tag, tag: another_recursive_tag (no-eol)

# Dump all the known Git objects into a file
  $ git rev-list --objects --all | git cat-file --batch-check='%(objectname) %(objecttype) %(rest)' | sort > $TESTTMP/new_object_list

# Ensure that there are no differences between the set of objects by diffing both object list files
  $ diff -w $TESTTMP/new_object_list $TESTTMP/object_list

# Capture all the known Git objects from the repo
  $ git rev-list --objects --all | git cat-file --batch-check='%(objectname) %(objecttype) %(rest)' | sort > $TESTTMP/object_list
# Import it into Mononoke
  $ cd "$TESTTMP"
  $ with_stripped_logs gitimport "$GIT_REPO_ORIGIN" --derive-hg --generate-bookmarks full-repo
  using repo "repo" repoid RepositoryId(0)
  GitRepo:$TESTTMP/origin/repo-git 2 of 2 commit(s) already exist
  Hg: Sha1(*): HgManifestId(HgNodeHash(Sha1(*))) (glob)
  Hg: Sha1(*): HgManifestId(HgNodeHash(Sha1(*))) (glob)
  Ref: "refs/heads/master_bookmark": Some(ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)))
  Ref: "refs/tags/another_recursive_tag": Some(ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)))
  Ref: "refs/tags/empty_tag": Some(ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)))
  Ref: "refs/tags/first_tag": Some(ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)))
  Ref: "refs/tags/recursive_tag": Some(ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)))
  Initializing repo: repo
  Initialized repo: repo
  All repos initialized. It took: * seconds (glob)
  Bookmark: "heads/master_bookmark": ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)) (already up-to-date)
  Bookmark: "tags/another_recursive_tag": ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)) (already up-to-date)
  Bookmark: "tags/empty_tag": ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)) (already up-to-date)
  Bookmark: "tags/first_tag": ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)) (already up-to-date)
  Bookmark: "tags/recursive_tag": ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)) (already up-to-date)

# Regenerate the Git repo out of the Mononoke repo
  $ mononoke_newadmin git-bundle create from-repo -R repo --output-location "$BUNDLE_PATH"
# Ensure that Git considers this a valid bundle
  $ cd $GIT_REPO
  $ git bundle verify -q $BUNDLE_PATH
  $TESTTMP/repo_bundle.bundle is okay

# Create a new empty folder for containing the repo
  $ cd "$TESTTMP"
  $ rm -rf git_client_repo
  $ git clone "$BUNDLE_PATH" git_client_repo
  Cloning into 'git_client_repo'...
