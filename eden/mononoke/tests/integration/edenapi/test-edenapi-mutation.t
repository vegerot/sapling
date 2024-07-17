# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"
  $ configure modern
  $ setconfig ui.ignorerevnum=false

  $ setconfig pull.httpcommitgraph2=true

Set up local hgrc and Mononoke config, with commit cloud, http pull and upload.
  $ export READ_ONLY_REPO=1
  $ export LOG=pull
  $ INFINITEPUSH_ALLOW_WRITES=true \
  >   setup_common_config
  $ cd $TESTTMP
  $ cat >> $HGRCPATH <<EOF
  > [extensions]
  > amend =
  > commitcloud =
  > infinitepush =
  > rebase =
  > remotenames =
  > share =
  > [commitcloud]
  > hostname = testhost
  > servicetype = local
  > servicelocation = $TESTTMP
  > owner_team = The Test Team
  > usehttpupload = True
  > [visibility]
  > enabled = True
  > [mutation]
  > record = True
  > enabled = True
  > date = 0 0
  > [remotefilelog]
  > reponame=repo
  > [pull]
  > httphashprefix = true
  > EOF
Custom smartlog
  $ function sl {
  >  hgedenapi log -G -T "{node|short} '{desc|firstline}' {join(mutations % '(Rewritten using {operation} into {join(successors % \'{node|short}\', \', \')})', ' ')}" --hidden
  > }

Initialize test repo.
  $ hginit_treemanifest repo
  $ cd repo
  $ mkcommit base_commit
  $ hg log -T '{short(node)}\n'
  8b2dca0c8a72


Import and start mononoke
  $ cd $TESTTMP
  $ hgclone_treemanifest ssh://user@dummy/repo client1 --noupdate --config clone.prefer-edenapi-clonedata=false
  DEBUG pull::httpbookmarks: edenapi fetched bookmarks: {'master': None}
  $ hgclone_treemanifest ssh://user@dummy/repo client2 --noupdate --config clone.prefer-edenapi-clonedata=false
  DEBUG pull::httpbookmarks: edenapi fetched bookmarks: {'master': None}
  $ blobimport repo/.hg repo
  $ start_and_wait_for_mononoke_server
Test mutations on client 1
  $ cd client1
  $ hgedenapi up 8b2dca0c8a72 -q
  DEBUG pull::httpbookmarks: edenapi fetched bookmarks: {'master': None}
  DEBUG pull::httphashlookup: edenapi hash lookups: ['8b2dca0c8a726d66bf26d47835a356cc4286facd']
  DEBUG pull::httpgraph: edenapi fetched 1 graph nodes
  DEBUG pull::httpgraph: edenapi fetched graph with known 0 draft commits
  $ hgedenapi cloud join -q
  $ mkcommitedenapi A
  $ hg log -T{node} -r .
  929f2b9071cf032d9422b3cce9773cbe1c574822 (no-eol)
  $ hgedenapi cloud upload -q
  $ hgedenapi debugapi -e commitmutations -i '["929f2b9071cf032d9422b3cce9773cbe1c574822"]'
  []
  $ hg metaedit -r . -m new_message
  $ hg log -T{node} -r .
  f643b098cd183f085ba3e6107b6867ca472e87d1 (no-eol)
  $ hgedenapi cloud upload -q
  $ hgedenapi debugapi -e commitmutations -i '["f643b098cd183f085ba3e6107b6867ca472e87d1"]'
  [{"op": "metaedit",
    "tz": 0,
    "time": 0,
    "user": [116,
             101,
             115,
             116],
    "split": [],
    "extras": [],
    "successor": bin("f643b098cd183f085ba3e6107b6867ca472e87d1"),
    "predecessors": [bin("929f2b9071cf032d9422b3cce9773cbe1c574822")]}]
  $ hgedenapi debugapi -e commitmutations -i '["929f2b9071cf032d9422b3cce9773cbe1c574822"]'
  []
Test phases from commitgraph
  $ hgedenapi debugapi -e commitgraph2 -i '["f643b098cd183f085ba3e6107b6867ca472e87d1", "929f2b9071cf032d9422b3cce9773cbe1c574822"]' -i '[]' --sort
  [{"hgid": bin("8b2dca0c8a726d66bf26d47835a356cc4286facd"),
    "parents": [],
    "is_draft": False},
   {"hgid": bin("929f2b9071cf032d9422b3cce9773cbe1c574822"),
    "parents": [bin("8b2dca0c8a726d66bf26d47835a356cc4286facd")],
    "is_draft": True},
   {"hgid": bin("f643b098cd183f085ba3e6107b6867ca472e87d1"),
    "parents": [bin("8b2dca0c8a726d66bf26d47835a356cc4286facd")],
    "is_draft": True}]
  $ hgedenapi debugapi -e commitmutations -i '["f643b098cd183f085ba3e6107b6867ca472e87d1", "929f2b9071cf032d9422b3cce9773cbe1c574822"]'
  [{"op": "metaedit",
    "tz": 0,
    "time": 0,
    "user": [116,
             101,
             115,
             116],
    "split": [],
    "extras": [],
    "successor": bin("f643b098cd183f085ba3e6107b6867ca472e87d1"),
    "predecessors": [bin("929f2b9071cf032d9422b3cce9773cbe1c574822")]}]
  $ sl
  @  f643b098cd18 'new_message'
  │
  │ x  929f2b9071cf 'A' (Rewritten using metaedit into f643b098cd18)
  ├─╯
  o  8b2dca0c8a72 'base_commit'
  

Test how they are propagated to client 2
  $ cd ../client2
  $ hgedenapi debugchangelog --migrate lazy
  $ hgedenapi pull -r f643b098cd18 -q
  DEBUG pull::httpbookmarks: edenapi fetched bookmarks: {'master': None}
  DEBUG pull::httphashlookup: edenapi hash lookups: ['f643b098cd183f085ba3e6107b6867ca472e87d1']
  DEBUG pull::httpgraph: edenapi fetched 2 graph nodes
  DEBUG pull::httpgraph: edenapi fetched graph with known 1 draft commits
  $ hgedenapi pull -r 929f2b9071cf -q
  DEBUG pull::httpbookmarks: edenapi fetched bookmarks: {'master': None}
  DEBUG pull::httphashlookup: edenapi hash lookups: ['929f2b9071cf032d9422b3cce9773cbe1c574822']
  DEBUG pull::httpgraph: edenapi fetched 1 graph nodes
  DEBUG pull::httpgraph: edenapi fetched graph with known 1 draft commits
  $ sl
  x  929f2b9071cf 'A' (Rewritten using metaedit into f643b098cd18)
  │
  │ o  f643b098cd18 'new_message'
  ├─╯
  o  8b2dca0c8a72 'base_commit'
  
