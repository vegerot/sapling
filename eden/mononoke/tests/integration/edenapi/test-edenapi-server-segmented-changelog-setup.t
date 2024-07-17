# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"

Set up local hgrc and Mononoke config.
  $ setup_common_config
  $ setup_configerator_configs
  $ cd $TESTTMP

Initialize test repo.
  $ hginit_treemanifest repo-hg
  $ cd repo-hg
  $ setup_hg_server

Populate test repo
  $ drawdag << EOS
  >   H
  >   |
  >   G
  >   |
  >   F
  >  /|
  > D |
  > | E
  > C |
  >  \|
  >   B
  >   |
  >   A
  > EOS
  $ hg bookmark -r "$H" "master_bookmark"
  $ hg log -G -T '{node} {desc}\n' -r "all()"
  o  06383dd46c9bcbca9300252b4b6cddad88f8af21 H
  │
  o  1b794c59b583e47686701d0142848e90a3a94a7d G
  │
  o    bb56d4161ee371c720dbc8b504810c62a22fe314 F
  ├─╮
  │ o  f585351a92f85104bff7c284233c338b10eb1df7 D
  │ │
  o │  49cb92066bfd0763fff729c354345650b7428554 E
  │ │
  │ o  26805aba1e600a82e93661149f2313866a221a7b C
  ├─╯
  o  112478962961147124edd43549aedd1a335e44bf B
  │
  o  426bada5c67598ca65036d57d9e4b64b0c1ce7a0 A
  

Blobimport test repo.
  $ cd ..
  $ blobimport repo-hg/.hg repo
  $ quiet default_setup_blobimport "blob_files"
  $ quiet segmented_changelog_tailer_reseed --repo repo --head=$F

Start up SaplingRemoteAPI server.
  $ cat >> "$TESTTMP/mononoke-config/repos/repo/server.toml" <<CONFIG
  > [segmented_changelog_config]
  > enabled=true
  > heads_to_include = [
  >    { bookmark = "master_bookmark" },
  > ]
  > CONFIG

  $ start_and_wait_for_mononoke_server
Create response.
  $ hgedenapi debugapi -e commithashtolocation -i "['$G']" -i "['$F','$D','$C','$B','$A','$F','$H']"
  [{"hgid": bin("bb56d4161ee371c720dbc8b504810c62a22fe314"),
    "result": {"Ok": {"distance": 1,
                      "descendant": bin("1b794c59b583e47686701d0142848e90a3a94a7d")}}},
   {"hgid": bin("f585351a92f85104bff7c284233c338b10eb1df7"),
    "result": {"Err": {"code": 0,
                       "message": "InternalError(InternalError(failed to compute the common descendant and distance for 86de925f9338cbc325f5ec1620b6556fb441d1e08466f65ae51930fae6abe120 with heads [ChangesetId(Blake2(a3bb22b98a3f75dbf14a07bcfe4200c4e5e94e3bef536b2f4165ba3db2b35d82))]: Programming(\"cannot convert 4 to x~n form (x must be in `H + parents(ancestors(H) & merge())` where H = 6) (trace: in seg 3-4[1], 4 has child seg (H5-7[2, 4]), child seg cannot be followed (4 is not p1))\")))"}}},
   {"hgid": bin("26805aba1e600a82e93661149f2313866a221a7b"),
    "result": {"Ok": {"distance": 1,
                      "descendant": bin("f585351a92f85104bff7c284233c338b10eb1df7")}}},
   {"hgid": bin("112478962961147124edd43549aedd1a335e44bf"),
    "result": {"Ok": {"distance": 2,
                      "descendant": bin("f585351a92f85104bff7c284233c338b10eb1df7")}}},
   {"hgid": bin("426bada5c67598ca65036d57d9e4b64b0c1ce7a0"),
    "result": {"Ok": {"distance": 3,
                      "descendant": bin("f585351a92f85104bff7c284233c338b10eb1df7")}}},
   {"hgid": bin("bb56d4161ee371c720dbc8b504810c62a22fe314"),
    "result": {"Ok": {"distance": 1,
                      "descendant": bin("1b794c59b583e47686701d0142848e90a3a94a7d")}}},
   {"hgid": bin("06383dd46c9bcbca9300252b4b6cddad88f8af21"),
    "result": {"Ok": None}}]

