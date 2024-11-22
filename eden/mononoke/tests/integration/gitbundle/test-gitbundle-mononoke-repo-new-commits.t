# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"
  $ REPOTYPE="blob_files"
  $ INFINITEPUSH_ALLOW_WRITES=true setup_common_config $REPOTYPE
  $ GIT_REPO_ORIGIN="${TESTTMP}/origin/repo-git"
  $ GIT_REPO="${TESTTMP}/repo-git"
  $ HG_REPO="${TESTTMP}/repo"
  $ BUNDLE_PATH="${TESTTMP}/repo_bundle.bundle"
  $ cat >> $HGRCPATH <<EOF
  > [extensions]
  > amend=
  > commitcloud=
  > EOF

# Enable commit cloud for the test
  $ cd $TESTTMP
  $ enable amend commitcloud

# Setup git repository
  $ mkdir -p "$GIT_REPO_ORIGIN"
  $ cd "$GIT_REPO_ORIGIN"
  $ git init -q
  $ echo "this is file1" > file1
  $ git add file1
  $ git commit -q -am "Add file1"
  $ git tag -a -m"new tag" first_tag
  $ echo "this is file2" > file2
  $ git add file2
  $ git commit -q -am "Add file2"
  $ git tag -a empty_tag -m ""
  $ cd "$TESTTMP"
  $ git clone "$GIT_REPO_ORIGIN"
  Cloning into 'repo-git'...
  done.
  $ cd repo-git
  $ git fetch "$GIT_REPO_ORIGIN" +refs/*:refs/* --prune -u
  From $TESTTMP/origin/repo-git
   - [deleted]         (none)     -> origin/master_bookmark
     (refs/remotes/origin/HEAD has become dangling)
  $ cd ..

# Import it into Mononoke
  $ cd "$TESTTMP"
  $ with_stripped_logs gitimport "$GIT_REPO" --derive-hg --generate-bookmarks full-repo
  using repo "repo" repoid RepositoryId(0)
  GitRepo:*repo-git commit 2 of 2 - Oid:* => Bid:* (glob)
  Hg: Sha1(8ce3eae44760b500bf3f2c3922a95dcd3c908e9e): HgManifestId(HgNodeHash(Sha1(009adbc8d457927d2e1883c08b0692bc45089839)))
  Hg: Sha1(e8615d6f149b876be0a2f30a1c5bf0c42bf8e136): HgManifestId(HgNodeHash(Sha1(d92f8d2d10e61e62f65acf25cdd638ea214f267f)))
  Ref: "refs/heads/master_bookmark": Some(ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)))
  Ref: "refs/tags/empty_tag": Some(ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)))
  Ref: "refs/tags/first_tag": Some(ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)))
  Initializing repo: repo
  Initialized repo: repo
  All repos initialized. It took: * seconds (glob)
  Bookmark: "heads/master_bookmark": ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)) (created)
  Bookmark: "tags/empty_tag": ChangesetId(Blake2(da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c)) (created)
  Bookmark: "tags/first_tag": ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)) (created)

# Regenerate the Git repo out of the Mononoke repo
  $ mononoke_admin git-bundle create from-repo -R repo --output-location "$BUNDLE_PATH"

# Create a new empty folder for containing the repo
  $ mkdir $TESTTMP/git_client_repo  
  $ cd "$TESTTMP"
  $ git clone "$BUNDLE_PATH" git_client_repo
  Cloning into 'git_client_repo'...
  $ cd git_client_repo
  $ git fetch "$BUNDLE_PATH" +refs/*:refs/* --prune -u
  From $TESTTMP/repo_bundle.bundle
   - [deleted]         (none)     -> origin/master_bookmark
     (refs/remotes/origin/HEAD has become dangling)
  $ cd ..

# Start Mononoke
  $ start_and_wait_for_mononoke_server
# Clone the repository
  $ cd "$TESTTMP"
  $ hg clone -q mono:repo "$HG_REPO" --config remotenames.selectivepulldefault=heads/master_bookmark
  $ cd "$HG_REPO"

# Add more commits to the HG repo
  $ echo "this is file3" > file3
  $ hg add
  adding file3
  $ hg commit -q -m "Add file3"
  $ echo "this is file4" > file4
  $ hg add
  adding file4
  $ hg commit -q -m "Add file4"
  $ echo "this is file5" > file5
  $ hg add
  adding file5
  $ hg commit -q -m "Add file5"

# Backup the created commits to commit cloud
  $ hg cloud backup
  commitcloud: head 'c2e143a98b79' hasn't been uploaded yet
  edenapi: queue 3 commits for upload
  edenapi: queue 3 files for upload
  edenapi: uploaded 3 files
  edenapi: queue 3 trees for upload
  edenapi: uploaded 3 trees
  edenapi: uploaded 3 changesets

# Get the bonsai changeset ID for the latest commit in the stack
  $ mononoke_admin convert -R repo -f hg -t bonsai $(hg whereami)
  19881757b04cb22f8c86ac8b30d0e7f8eb26348ee271ff6c1f0f9b4fabb266ac

# Generate a git bundle for the changes made in the draft commit
  $ mononoke_admin git-bundle create from-repo -R repo --output-location "$BUNDLE_PATH" --have-heads da93dc81badd8d407db0f3219ec0ec78f1ef750ebfa95735bb483310371af80c --included-refs-with-value heads/master_bookmark=19881757b04cb22f8c86ac8b30d0e7f8eb26348ee271ff6c1f0f9b4fabb266ac,heads/non_existent_ref=19881757b04cb22f8c86ac8b30d0e7f8eb26348ee271ff6c1f0f9b4fabb266ac

# Ensure that Git considers this a valid bundle
  $ cd $GIT_REPO
  $ git bundle verify -q $BUNDLE_PATH
  $TESTTMP/repo_bundle.bundle is okay

# Apply the bundle on the existing Git repo
  $ cd $TESTTMP/git_client_repo  
  $ git pull -q "$BUNDLE_PATH"

# Get the repository log and verify that it has all the changes and draft commits from Mononoke
# Note the local master_bookmark points at the 5th commit while the remote master_bookmark points at the 2nd commit
# This indicates that if the repo was pushed, it would push the three draft commits which is exactly
# what we expected
  $ git log --pretty=format:"%h %an %s %D" --stat
  e959bd2 test Add file5 HEAD -> master_bookmark
   file5 | 1 +
   1 file changed, 1 insertion(+)
  
  48a5147 test Add file4 
   file4 | 1 +
   1 file changed, 1 insertion(+)
  
  9250ce8 test Add file3 
   file3 | 1 +
   1 file changed, 1 insertion(+)
  
  e8615d6 mononoke Add file2 tag: empty_tag
   file2 | 1 +
   1 file changed, 1 insertion(+)
  
  8ce3eae mononoke Add file1 tag: first_tag
   file1 | 1 +
   1 file changed, 1 insertion(+)
