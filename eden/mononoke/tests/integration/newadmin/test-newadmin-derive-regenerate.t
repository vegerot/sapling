# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"
  $ ENABLED_DERIVED_DATA='["git_trees", "filenodes", "hgchangesets"]' setup_common_config
  $ GIT_REPO="${TESTTMP}/repo-git"
  $ HG_REPO="${TESTTMP}/repo"

# Setup git repository
  $ mkdir "$GIT_REPO"
  $ cd "$GIT_REPO"
  $ git init -q
  $ echo "this is file1" > file1
  $ git add file1
  $ git commit -am "Add file1"
  [master (root-commit) 8ce3eae] Add file1
   1 file changed, 1 insertion(+)
   create mode 100644 file1
  $ git log
  commit 8ce3eae44760b500bf3f2c3922a95dcd3c908e9e
  Author: mononoke <mononoke@mononoke>
  Date:   Sat Jan 1 00:00:00 2000 +0000
  
      Add file1

  $ cd "$TESTTMP"
  $ git clone repo-git repo-git-clone
  Cloning into 'repo-git-clone'...
  done.
  $ cd "$TESTTMP/repo-git"
  $ git checkout --orphan another_committer
  Switched to a new branch 'another_committer'
  $ echo "this is file1" > file1
  $ git add file1
  $ export GIT_COMMITTER_NAME="second_committer"
  $ export GIT_COMMITTER_EMAIL="second_committer@fb.com"
  $ export GIT_COMMITTER_DATE="1000000000"
  $ git_set_only_author commit -am "Add file1"
  [another_committer (root-commit) 69a2653] Add file1
   Author: mononoke <mononoke@mononoke>
   1 file changed, 1 insertion(+)
   create mode 100644 file1
  $ git log --pretty=fuller
  commit 69a265312a2c29cdf5667ff401d895a66e6ac02a
  Author:     mononoke <mononoke@mononoke>
  AuthorDate: Sat Jan 1 00:00:00 2000 +0000
  Commit:     second_committer <second_committer@fb.com>
  CommitDate: Sun Sep 9 01:46:40 2001 +0000
  
      Add file1

# Import it into Mononoke
  $ cd "$TESTTMP"
  $ with_stripped_logs gitimport "$GIT_REPO" --derive-hg full-repo
  using repo "repo" repoid RepositoryId(0)
  GitRepo:$TESTTMP/repo-git commit 2 of 2 - Oid:69a26531 => Bid:1213979c
  Hg: Sha1(8ce3eae44760b500bf3f2c3922a95dcd3c908e9e): HgManifestId(HgNodeHash(Sha1(009adbc8d457927d2e1883c08b0692bc45089839)))
  Hg: Sha1(69a265312a2c29cdf5667ff401d895a66e6ac02a): HgManifestId(HgNodeHash(Sha1(009adbc8d457927d2e1883c08b0692bc45089839)))
  Ref: "refs/heads/another_committer": Some(ChangesetId(Blake2(1213979c6023f23e70dbe8845d773078ac1e0506bc2ab98382a329da0cb379a7)))
  Ref: "refs/heads/master": Some(ChangesetId(Blake2(032cd4dce0406f1c1dd1362b6c3c9f9bdfa82f2fc5615e237a890be4fe08b044)))

# Check hg hash before overwriting
  $ mononoke_newadmin convert -R repo --from bonsai --to hg 1213979c6023f23e70dbe8845d773078ac1e0506bc2ab98382a329da0cb379a7 --derive
  52aee0f873361473bbb29cbce0c1ba5d0c1a2c5e

# Now rederive HG_SET_COMMITTER_EXTRA=true. This changes hg hash, so we need to run with --rederive and make sure
# hg hash was overwritten.
  $ HG_SET_COMMITTER_EXTRA=true ENABLED_DERIVED_DATA='["git_trees", "filenodes", "hgchangesets"]' setup_common_config

# If we call derive without the rederive flag, we have the same hg hash as before
  $ mononoke_newadmin derived-data -R repo derive -T hgchangesets -i 1213979c6023f23e70dbe8845d773078ac1e0506bc2ab98382a329da0cb379a7
  $ mononoke_newadmin convert -R repo --from bonsai --to hg 1213979c6023f23e70dbe8845d773078ac1e0506bc2ab98382a329da0cb379a7
  52aee0f873361473bbb29cbce0c1ba5d0c1a2c5e

# With rederive, we can see that the hash is now different as expected
  $ mononoke_newadmin derived-data -R repo derive -T hgchangesets -i 1213979c6023f23e70dbe8845d773078ac1e0506bc2ab98382a329da0cb379a7 --rederive
  $ mononoke_newadmin convert -R repo --from bonsai --to hg 1213979c6023f23e70dbe8845d773078ac1e0506bc2ab98382a329da0cb379a7
  c4c28fe2943cad9b4fed5a6982d3ffc0a83b4e7e
