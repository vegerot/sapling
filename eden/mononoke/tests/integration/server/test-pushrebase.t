# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.
#require slow

  $ . "${TEST_FIXTURES}/library.sh"
  $ setconfig ui.ignorerevnum=false
  $ setconfig push.edenapi=true
  $ ENABLE_API_WRITES=1 BLOB_TYPE="blob_files" default_setup --scuba-dataset "file://$TESTTMP/log.json"
  hg repo
  o  C [draft;rev=2;26805aba1e60]
  │
  o  B [draft;rev=1;112478962961]
  │
  o  A [draft;rev=0;426bada5c675]
  $
  blobimporting
  starting Mononoke
  cloning repo in hg client 'repo2'


Pushrebase commit 1
  $ hg up -q "min(all())"
  $ echo 1 > 1 && hg add 1 && hg ci -m 1
  $ hgedenapi push -r . --to master_bookmark
  pushing rev a0c9c5791058 to destination https://localhost:*/edenapi/ bookmark master_bookmark (glob)
  edenapi: queue 1 commit for upload
  edenapi: queue 1 file for upload
  edenapi: uploaded 1 file
  edenapi: queue 1 tree for upload
  edenapi: uploaded 1 tree
  edenapi: uploaded 1 changeset
  pushrebasing stack (426bada5c675, a0c9c5791058] (1 commit) to remote bookmark master_bookmark
  2 files updated, 0 files merged, 0 files removed, 0 files unresolved
  updated remote bookmark master_bookmark to c2e526aacb51

  $ log -r "all()"
  @  1 [public;rev=4;c2e526aacb51] default/master_bookmark
  │
  o  C [public;rev=2;26805aba1e60]
  │
  o  B [public;rev=1;112478962961]
  │
  o  A [public;rev=0;426bada5c675]
  $

Pushrebased commit 1 over commits B and C (thus the distance should be 2).
  $ jq < "$TESTTMP/log.json" '.int.pushrebase_distance | numbers' | tail -n 1
  2

Check that the filenode for 1 does not point to the draft commit in a new clone
  $ cd ..
  $ hgclone_treemanifest ssh://user@dummy/repo-hg repo3 --noupdate --config extensions.remotenames= -q
  $ cd repo3
  $ setup_hg_client
  $ cat >> .hg/hgrc <<EOF
  > [extensions]
  > pushrebase =
  > remotenames =
  > EOF

  $ hgmn pull -r master_bookmark
  pulling from mononoke://$LOCALIP:$LOCAL_PORT/repo
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  $ hgmn up master_bookmark
  4 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hgmn debugsh -c 'ui.write("%s\n" % s.node.hex(repo["."].filectx("1").getnodeinfo()[2]))'
  c2e526aacb5100b7c1ddb9b711d2e012e6c69cda
  $ cd ../repo2

Push rebase fails with conflict in the bottom of the stack
  $ hg up -q "min(all())"
  $ echo 1 > 1 && hg add 1 && hg ci -m 1
  $ echo 2 > 2 && hg add 2 && hg ci -m 2
  $ hgedenapi push -r . --to master_bookmark
  pushing rev 0c67ec8c24b9 to destination https://localhost:*/edenapi/ bookmark master_bookmark (glob)
  edenapi: queue 1 commit for upload
  edenapi: queue 1 file for upload
  edenapi: uploaded 1 file
  edenapi: queue 1 tree for upload
  edenapi: uploaded 1 tree
  edenapi: uploaded 1 changeset
  pushrebasing stack (426bada5c675, 0c67ec8c24b9] (2 commits) to remote bookmark master_bookmark
  abort: Server error: Conflicts while pushrebasing: [PushrebaseConflict { left: NonRootMPath("1"), right: NonRootMPath("1") }]
  [255]
  $ hg hide -r ".^ + ." -q


Push rebase fails with conflict in the top of the stack
  $ hg up -q "min(all())"
  $ echo 2 > 2 && hg add 2 && hg ci -m 2
  $ echo 1 > 1 && hg add 1 && hg ci -m 1
  $ hgedenapi push -r . --to master_bookmark
  pushing rev 8d2ff619947e to destination https://localhost:*/edenapi/ bookmark master_bookmark (glob)
  edenapi: queue 2 commits for upload
  edenapi: queue 0 files for upload
  edenapi: queue 2 trees for upload
  edenapi: uploaded 2 trees
  edenapi: uploaded 2 changesets
  pushrebasing stack (426bada5c675, 8d2ff619947e] (2 commits) to remote bookmark master_bookmark
  abort: Server error: Conflicts while pushrebasing: [PushrebaseConflict { left: NonRootMPath("1"), right: NonRootMPath("1") }]
  [255]
  $ hg hide -r ".^ + ." -q


Push stack
  $ hg up -q "min(all())"
  $ echo 3 > 3 && hg add 3 && hg ci -m 3
  $ echo 4 > 4 && hg add 4 && hg ci -m 4
  $ hgedenapi push -r . --to master_bookmark
  pushing rev 7a68f123d810 to destination https://localhost:*/edenapi/ bookmark master_bookmark (glob)
  edenapi: queue 2 commits for upload
  edenapi: queue 2 files for upload
  edenapi: uploaded 2 files
  edenapi: queue 2 trees for upload
  edenapi: uploaded 2 trees
  edenapi: uploaded 2 changesets
  pushrebasing stack (426bada5c675, 7a68f123d810] (2 commits) to remote bookmark master_bookmark
  3 files updated, 0 files merged, 0 files removed, 0 files unresolved
  updated remote bookmark master_bookmark to 4f5a4463b24b
  $ hgmn up -q master_bookmark
  $ log -r "all()"
  @  4 [public;rev=11;4f5a4463b24b] default/master_bookmark
  │
  o  3 [public;rev=10;7796136324ad]
  │
  o  1 [public;rev=4;c2e526aacb51]
  │
  o  C [public;rev=2;26805aba1e60]
  │
  o  B [public;rev=1;112478962961]
  │
  o  A [public;rev=0;426bada5c675]
  $

Pushrebased commits {3, 4} over commits {B, C, 1} (thus the distance should be 3).
  $ jq < "$TESTTMP/log.json" '.int.pushrebase_distance | numbers' | tail -n 1
  3

Push fast-forward
  $ hg up master_bookmark
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ echo 5 > 5 && hg add 5 && hg ci -m 5
  $ hgedenapi push -r . --to master_bookmark
  pushing rev 59e5396444cf to destination https://localhost:*/edenapi/ bookmark master_bookmark (glob)
  edenapi: queue 1 commit for upload
  edenapi: queue 1 file for upload
  edenapi: uploaded 1 file
  edenapi: queue 1 tree for upload
  edenapi: uploaded 1 tree
  edenapi: uploaded 1 changeset
  pushrebasing stack (4f5a4463b24b, 59e5396444cf] (1 commit) to remote bookmark master_bookmark
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved
  updated remote bookmark master_bookmark to 59e5396444cf
  $ log -r "all()"
  @  5 [public;rev=12;59e5396444cf] default/master_bookmark
  │
  o  4 [public;rev=11;4f5a4463b24b]
  │
  o  3 [public;rev=10;7796136324ad]
  │
  o  1 [public;rev=4;c2e526aacb51]
  │
  o  C [public;rev=2;26805aba1e60]
  │
  o  B [public;rev=1;112478962961]
  │
  o  A [public;rev=0;426bada5c675]
  $
  $ jq < "$TESTTMP/log.json" '.int.pushrebase_distance | numbers' | tail -n 1
  0


Push with no new commits
  $ hgedenapi push -r . --to master_bookmark
  pushing rev 59e5396444cf to destination https://localhost:*/edenapi/ bookmark master_bookmark (glob)
  moving remote bookmark master_bookmark from 59e5396444cf to 59e5396444cf
  $ log -r "."
  @  5 [public;rev=12;59e5396444cf] default/master_bookmark
  │
  ~

Push a merge commit with both parents not ancestors of destination bookmark
  $ hg up -q 1
  $ echo 6 > 6 && hg add 6 && hg ci -m 6
  $ hg up -q 1
  $ echo 7 > 7 && hg add 7 && hg ci -m 7
  $ hg merge -q -r 13 && hg ci -m "merge 6 and 7"
  $ log -r "all()"
  @    merge 6 and 7 [draft;rev=15;fad460d85200]
  ├─╮
  │ o  7 [draft;rev=14;299aa3fbbd3f]
  │ │
  o │  6 [draft;rev=13;55337b4265b3]
  ├─╯
  │ o  5 [public;rev=12;59e5396444cf] default/master_bookmark
  │ │
  │ o  4 [public;rev=11;4f5a4463b24b]
  │ │
  │ o  3 [public;rev=10;7796136324ad]
  │ │
  │ o  1 [public;rev=4;c2e526aacb51]
  │ │
  │ o  C [public;rev=2;26805aba1e60]
  ├─╯
  o  B [public;rev=1;112478962961]
  │
  o  A [public;rev=0;426bada5c675]
  $

  $ hgedenapi push -r . --to master_bookmark
  fallback reason: merge commit is not supported by EdenApi push yet
  pushing rev fad460d85200 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master_bookmark
  $ hgmn up master_bookmark -q && hg hide -r "13+14+15" -q
  $ log -r "all()"
  @    merge 6 and 7 [public;rev=18;4a0002072071] default/master_bookmark
  ├─╮
  * (glob)
  │ │
  * (glob)
  ├─╯
  o  5 [public;rev=12;59e5396444cf]
  │
  o  4 [public;rev=11;4f5a4463b24b]
  │
  o  3 [public;rev=10;7796136324ad]
  │
  o  1 [public;rev=4;c2e526aacb51]
  │
  o  C [public;rev=2;26805aba1e60]
  │
  o  B [public;rev=1;112478962961]
  │
  o  A [public;rev=0;426bada5c675]
  $
  $ jq < "$TESTTMP/log.json" '.int.pushrebase_distance | numbers' | tail -n 1
  5


Previously commits below were testing pushrebasing over merge.
Keep them in place to not change the output for all the tests below
  $ hgmn up 11 -q
  $ echo 8 > 8 && hg add 8 && hg ci -m 8
  $ hgmn up master_bookmark -q

Push-rebase of a commit with p2 being the ancestor of the destination bookmark
- Do some preparatory work
  $ echo 9 > 9 && hg add 9 && hg ci -m 9
  $ echo 10 > 10 && hg add 10 && hg ci -m 10
  $ echo 11 > 11 && hg add 11 && hg ci -m 11
  $ hgedenapi push -r . --to master_bookmark -q
  $ hgmn up .^^ && echo 12 > 12 && hg add 12 && hg ci -m 12
  0 files updated, 0 files merged, 2 files removed, 0 files unresolved
  $ hg log -r master_bookmark -T '{node}\n'
  589551466f2555a4d90ca544b23273a2eed21f9d

  $ hg merge -qr 21 && hg ci -qm "merge 10 and 12"
  $ hg phase -r $(hg log -r . -T "{p1node}")
  cd5aac4439e50d4329539ac117bfb3e35d7fb74b: draft
  $ hg phase -r $(hg log -r . -T "{p2node}")
  c573a92e1179f7367f4e4a51689d097bb84842ab: public
  $ hg log -r master_bookmark -T '{node}\n'
  589551466f2555a4d90ca544b23273a2eed21f9d

- Actually test the push
  $ hgedenapi push -r . --to master_bookmark
  fallback reason: merge commit is not supported by EdenApi push yet
  pushing rev e3db177db1d1 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master_bookmark
  $ hg hide -r . -q && hgmn up master_bookmark -q
  $ hg log -r master_bookmark -T '{node}\n'
  eb388b759fde98ed5b1e05fd2da5309f3762c2fd
Test creating a bookmark on a public commit
  $ hgedenapi push --rev 25 --to master_bookmark_2 --create
  pushing rev eb388b759fde to destination https://localhost:*/edenapi/ bookmark master_bookmark_2 (glob)
  creating remote bookmark master_bookmark_2
  $ log -r "20::"
  @    merge 10 and 12 [public;rev=25;eb388b759fde] default/master_bookmark default/master_bookmark_2
  ├─╮
  │ o  12 [public;rev=23;cd5aac4439e5]
  │ │
  o │  11 [public;rev=22;589551466f25]
  │ │
  o │  10 [public;rev=21;c573a92e1179]
  ├─╯
  o  9 [public;rev=20;2f7cc50dc4e5]
  │
  ~

Test a non-forward push
  $ hgmn up 22 -q
  $ log -r "20::"
  o    merge 10 and 12 [public;rev=25;eb388b759fde] default/master_bookmark default/master_bookmark_2
  ├─╮
  │ o  12 [public;rev=23;cd5aac4439e5]
  │ │
  @ │  11 [public;rev=22;589551466f25]
  │ │
  o │  10 [public;rev=21;c573a92e1179]
  ├─╯
  o  9 [public;rev=20;2f7cc50dc4e5]
  │
  ~
  $ hgedenapi push --force -r . --to master_bookmark_2 --non-forward-move --pushvar NON_FAST_FORWARD=true
  pushing rev 589551466f25 to destination https://localhost:*/edenapi/ bookmark master_bookmark_2 (glob)
  moving remote bookmark master_bookmark_2 from eb388b759fde to 589551466f25
  $ log -r "20::"
  o    merge 10 and 12 [public;rev=25;eb388b759fde] default/master_bookmark
  ├─╮
  │ o  12 [public;rev=23;cd5aac4439e5]
  │ │
  @ │  11 [public;rev=22;589551466f25] default/master_bookmark_2
  │ │
  o │  10 [public;rev=21;c573a92e1179]
  ├─╯
  o  9 [public;rev=20;2f7cc50dc4e5]
  │
  ~

Test deleting a bookmark
  $ hgedenapi push --delete master_bookmark_2
  deleting remote bookmark master_bookmark_2
  $ log -r "20::"
  o    merge 10 and 12 [public;rev=25;eb388b759fde] default/master_bookmark
  ├─╮
  │ o  12 [public;rev=23;cd5aac4439e5]
  │ │
  @ │  11 [public;rev=22;589551466f25]
  │ │
  o │  10 [public;rev=21;c573a92e1179]
  ├─╯
  o  9 [public;rev=20;2f7cc50dc4e5]
  │
  ~

Test creating a bookmark and new head
  $ echo draft > draft && hg add draft && hg ci -m draft
  $ hgedenapi push -r . --to newbook --create
  pushing rev 7a037594e202 to destination https://localhost:*/edenapi/ bookmark newbook (glob)
  edenapi: queue 1 commit for upload
  edenapi: queue 1 file for upload
  edenapi: uploaded 1 file
  edenapi: queue 1 tree for upload
  edenapi: uploaded 1 tree
  edenapi: uploaded 1 changeset
  creating remote bookmark newbook

Test non-fast-forward force pushrebase
  $ hgmn up -qr 20
  $ echo Aeneas > was_a_lively_fellow && hg ci -qAm 26
  $ log -r "20::"
  @  26 [draft;rev=27;4899f9112d9b]
  │
  │ o  draft [public;rev=26;7a037594e202] default/newbook
  │ │
  │ │ o  merge 10 and 12 [public;rev=25;eb388b759fde] default/master_bookmark
  │ ╭─┤
  │ │ o  12 [public;rev=23;cd5aac4439e5]
  ├───╯
  │ o  11 [public;rev=22;589551466f25]
  │ │
  │ o  10 [public;rev=21;c573a92e1179]
  ├─╯
  o  9 [public;rev=20;2f7cc50dc4e5]
  │
  ~
-- we don't need to pass --pushvar NON_FAST_FORWARD if we're doing a force pushrebase
  $ hgedenapi push -r . -f --to newbook
  pushing rev 4899f9112d9b to destination https://localhost:*/edenapi/ bookmark newbook (glob)
  edenapi: queue 1 commit for upload
  edenapi: queue 1 file for upload
  edenapi: uploaded 1 file
  edenapi: queue 1 tree for upload
  edenapi: uploaded 1 tree
  edenapi: uploaded 1 changeset
  moving remote bookmark newbook from 7a037594e202 to 4899f9112d9b
-- "20 draft newbook" gets moved to 26 and 20 gets hidden.
  $ log -r "20::"
  @  26 [public;rev=27;4899f9112d9b] default/newbook
  │
  │ o    merge 10 and 12 [public;rev=25;eb388b759fde] default/master_bookmark
  │ ├─╮
  │ │ o  12 [public;rev=23;cd5aac4439e5]
  ├───╯
  │ o  11 [public;rev=22;589551466f25]
  │ │
  │ o  10 [public;rev=21;c573a92e1179]
  ├─╯
  o  9 [public;rev=20;2f7cc50dc4e5]
  │
  ~

-- Check that pulling a force pushrebase has good linknodes.
  $ cd ../repo3
  $ hgmn pull -r newbook
  pulling from mononoke://$LOCALIP:$LOCAL_PORT/repo
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  $ hgmn up newbook
  7 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hgmn debugsh -c 'ui.write("%s\n" % s.node.hex(repo["."].filectx("was_a_lively_fellow").getnodeinfo()[2]))'
  4899f9112d9b79c3ecbc343169db37fbe1efdd20
  $ cd ../repo2

Check that a force pushrebase with mutation markers.
  $ echo SPARTACUS > sum_ego && hg ci -qAm 27
  $ echo SPARTACUS! > sum_ego && hg amend --config mutation.enabled=true --config mutation.record=true
  $ hgedenapi push -r . -f --to newbook --config push.check-mutation=true
  pushing rev * to destination https://localhost:*/edenapi/ bookmark newbook (glob)
  edenapi: queue 1 commit for upload
  edenapi: queue 1 file for upload
  edenapi: uploaded 1 file
  edenapi: queue 1 tree for upload
  edenapi: uploaded 1 tree
  edenapi: uploaded 1 changeset
  abort: forced push blocked because commit * contains mutation metadata (glob)
  (use 'hg amend --config mutation.record=false' to remove the metadata)
  [255]

Check that we can replace a file with a directory
  $ cd "$TESTTMP/repo2"
  $ hgmn up default/newbook -q
  $ hg rm A -q
  $ mkdir A
  $ echo hello > A/hello
  $ hgmn add A/hello -q
  $ hgmn ci -qm "replace a file with a dir"
  $ hgedenapi push --to newbook
  pushing rev 4e5fec14573f to destination https://localhost:*/edenapi/ bookmark newbook (glob)
  edenapi: queue 1 commit for upload
  edenapi: queue 1 file for upload
  edenapi: uploaded 1 file
  edenapi: queue 2 trees for upload
  edenapi: uploaded 2 trees
  edenapi: uploaded 1 changeset
  pushrebasing stack (4899f9112d9b, 4e5fec14573f] (1 commit) to remote bookmark newbook
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved
  updated remote bookmark newbook to 4e5fec14573f

  $ ls A
  hello
  $ log -r "30"
  @  replace a file with a dir [public;rev=30;4e5fec14573f] default/newbook
  │
  ~
