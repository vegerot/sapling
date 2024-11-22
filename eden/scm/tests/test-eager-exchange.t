
#require no-eden

  $ configure modern

  $ setconfig paths.default=test:e1 ui.traceback=1
  $ export LOG=sapling::eagerpeer=trace,eagerepo::api=trace

Disable SSH:

  $ setconfig ui.ssh=false

Prepare Repo:

  $ newremoterepo
  $ setconfig paths.default=test:e1
  $ drawdag << 'EOS'
  >   D
  >   |
  > B C  # C/T/A=2
  > |/
  > A    # A/T/A=1
  > EOS

Push:

  $ hg push -r $C --to master --create
  pushing rev 178c10ffbc2f to destination test:e1 bookmark master
  DEBUG eagerepo::api: bookmarks master
  DEBUG eagerepo::api: commit_known 178c10ffbc2f92d5407c14478ae9d9dea81f232e
  DEBUG sapling::eagerpeer: heads = []
  searching for changes
  DEBUG eagerepo::api: commit_known 748104bd5058bf2c386d074d8dcf2704855380f6
  DEBUG eagerepo::api: bookmarks master
  DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict()
  TRACE sapling::eagerpeer: adding   blob 005d992c5dcf32993668f7cede29d296c494a5d9
  TRACE sapling::eagerpeer: adding   blob f976da1d0df2256cde08db84261621d5e92f77be
  TRACE sapling::eagerpeer: adding   tree 4c28a8a0e46c55df521ea9d682b5b6b8a91031a2
  TRACE sapling::eagerpeer: adding   tree 6161efd5db4f6d976d6aba647fa77c12186d3179
  TRACE sapling::eagerpeer: adding commit 748104bd5058bf2c386d074d8dcf2704855380f6
  TRACE sapling::eagerpeer: adding   blob a2e456504a5e61f763f1a0b36a6c247c7541b2b3
  TRACE sapling::eagerpeer: adding   blob d85e50a0f00eee8211502158e93772aec5dc3d63
  TRACE sapling::eagerpeer: adding   tree 319bc9670b2bff0a75b8b2dfa78867bf1f8d7aec
  TRACE sapling::eagerpeer: adding   tree 0ccf968573574750913fcee533939cc7ebe7327d
  TRACE sapling::eagerpeer: adding commit 178c10ffbc2f92d5407c14478ae9d9dea81f232e
  DEBUG sapling::eagerpeer: flushed
  DEBUG eagerepo::api: bookmarks master
  DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict()
  DEBUG sapling::eagerpeer: flushed
  DEBUG sapling::eagerpeer: pushkey bookmarks 'master': '' => '178c10ffbc2f92d5407c14478ae9d9dea81f232e' (success)
  exporting bookmark master
  DEBUG eagerepo::api: bookmarks master
  DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict([('master', '178c10ffbc2f92d5407c14478ae9d9dea81f232e')])

  $ hg push -r $B --allow-anon
  pushing to test:e1
  DEBUG eagerepo::api: bookmarks master
  DEBUG eagerepo::api: commit_known 178c10ffbc2f92d5407c14478ae9d9dea81f232e, 99dac869f01e09fe3d501fa645ea524af80d498f
  searching for changes
  DEBUG eagerepo::api: bookmarks master
  DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict([('master', '178c10ffbc2f92d5407c14478ae9d9dea81f232e')])
  TRACE sapling::eagerpeer: adding   blob 35e7525ce3a48913275d7061dd9a867ffef1e34d
  TRACE sapling::eagerpeer: adding   tree d8dc55ad2b89cdc0f1ee969e5d79bd1eaddb5b43
  TRACE sapling::eagerpeer: adding commit 99dac869f01e09fe3d501fa645ea524af80d498f
  DEBUG sapling::eagerpeer: flushed
  DEBUG eagerepo::api: bookmarks master
  DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict([('master', '178c10ffbc2f92d5407c14478ae9d9dea81f232e')])

  $ hg push -r $D --to master
  pushing rev 23d30dc6b703 to destination test:e1 bookmark master
  DEBUG eagerepo::api: bookmarks master
  DEBUG eagerepo::api: commit_known 178c10ffbc2f92d5407c14478ae9d9dea81f232e, 23d30dc6b70380b2d939023947578ae0e0198999
  searching for changes
  DEBUG eagerepo::api: bookmarks master
  DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict([('master', '178c10ffbc2f92d5407c14478ae9d9dea81f232e')])
  TRACE sapling::eagerpeer: adding   blob 4eec8cfdabce9565739489483b6ad93ef7657ea9
  TRACE sapling::eagerpeer: adding   tree 4a38281d93dab71e695b39f85bdfbac0ce78011d
  TRACE sapling::eagerpeer: adding commit 23d30dc6b70380b2d939023947578ae0e0198999
  DEBUG sapling::eagerpeer: flushed
  DEBUG eagerepo::api: bookmarks master
  DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict([('master', '178c10ffbc2f92d5407c14478ae9d9dea81f232e')])
  DEBUG sapling::eagerpeer: flushed
  DEBUG sapling::eagerpeer: pushkey bookmarks 'master': '178c10ffbc2f92d5407c14478ae9d9dea81f232e' => '23d30dc6b70380b2d939023947578ae0e0198999' (success)
  updating bookmark master
  DEBUG eagerepo::api: bookmarks master
  DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict([('master', '23d30dc6b70380b2d939023947578ae0e0198999')])

Pull (non-lazy):

    $ newremoterepo
    $ setconfig paths.default=test:e1
    $ hg debugchangelog --migrate revlog
    $ LOG= hg pull -B master -r $B
    pulling from test:e1
    fetching revlog data for 4 commits
    $ LOG= hg log -Gr 'all()' -T '{desc} {remotenames}'
    o  D remote/master
    │
    o  C
    │
    │ o  B
    ├─╯
    o  A

    $ newremoterepo
    $ setconfig paths.default=test:e1
    $ hg debugchangelog --migrate fullsegments
    $ LOG= hg pull -B master -r $B
    pulling from test:e1
    fetching revlog data for 4 commits
    $ LOG= hg log -Gr 'all()' -T '{desc} {remotenames}'
    o  B
    │
    │ o  D remote/master
    │ │
    │ o  C
    ├─╯
    o  A

Pull (lazy):

    for cltype in ["lazytext", "lazy"]:
      $ newremoterepo
      $ setconfig paths.default=test:e1
      $ hg debugchangelog --migrate $(py cltype)
      $ hg pull -B master
      pulling from test:e1
      DEBUG eagerepo::api: bookmarks master
      DEBUG eagerepo::api: bookmarks master
      DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict([('master', '23d30dc6b70380b2d939023947578ae0e0198999')])
      DEBUG eagerepo::api: bookmarks master
      DEBUG eagerepo::api: commit_known 
      DEBUG eagerepo::api: commit_graph 23d30dc6b70380b2d939023947578ae0e0198999 
      DEBUG eagerepo::api: commit_mutations 178c10ffbc2f92d5407c14478ae9d9dea81f232e, 23d30dc6b70380b2d939023947578ae0e0198999, 748104bd5058bf2c386d074d8dcf2704855380f6

      $ hg pull -r $B
      pulling from test:e1
      DEBUG eagerepo::api: bookmarks master
      DEBUG eagerepo::api: commit_known 99dac869f01e09fe3d501fa645ea524af80d498f
      TRACE sapling::eagerpeer: known 99dac869f01e09fe3d501fa645ea524af80d498f: True
      DEBUG eagerepo::api: bookmarks master
      DEBUG sapling::eagerpeer: listkeyspatterns(bookmarks, ['master']) = sortdict([('master', '23d30dc6b70380b2d939023947578ae0e0198999')])
      DEBUG eagerepo::api: bookmarks master
      DEBUG eagerepo::api: commit_known 23d30dc6b70380b2d939023947578ae0e0198999
      searching for changes
      DEBUG eagerepo::api: commit_graph 99dac869f01e09fe3d501fa645ea524af80d498f 23d30dc6b70380b2d939023947578ae0e0198999
      DEBUG eagerepo::api: commit_mutations 99dac869f01e09fe3d501fa645ea524af80d498f

      $ hg debugmakepublic -r $B

      $ hg log -Gr 'all()' -T '{desc} {remotenames}'
      DEBUG eagerepo::api: revlog_data 99dac869f01e09fe3d501fa645ea524af80d498f, 23d30dc6b70380b2d939023947578ae0e0198999, 178c10ffbc2f92d5407c14478ae9d9dea81f232e, 748104bd5058bf2c386d074d8dcf2704855380f6
      TRACE eagerepo::api:  found: 99dac869f01e09fe3d501fa645ea524af80d498f, 94 bytes
      TRACE eagerepo::api:  found: 23d30dc6b70380b2d939023947578ae0e0198999, 94 bytes
      TRACE eagerepo::api:  found: 178c10ffbc2f92d5407c14478ae9d9dea81f232e, 98 bytes
      TRACE eagerepo::api:  found: 748104bd5058bf2c386d074d8dcf2704855380f6, 98 bytes
      o  B* (glob)
      │
      │ o  D remote/master
      │ │
      │ o  C
      ├─╯
      o  A
  
Trigger file and tree downloading:

  $ hg cat -r $B B A >out 2>err
  $ cat err out
  DEBUG eagerepo::api: trees d8dc55ad2b89cdc0f1ee969e5d79bd1eaddb5b43 Some(TreeAttributes { manifest_blob: true, parents: true, child_metadata: false, augmented_trees: false })
  TRACE eagerepo::api:  found: d8dc55ad2b89cdc0f1ee969e5d79bd1eaddb5b43, 170 bytes
  DEBUG eagerepo::api: files_attrs FileSpec { key: Key { path: RepoPathBuf("A"), hgid: HgId("005d992c5dcf32993668f7cede29d296c494a5d9") }, attrs: FileAttributes { content: true, aux_data: false } }
  TRACE eagerepo::api:  found: 005d992c5dcf32993668f7cede29d296c494a5d9, 41 bytes
  DEBUG eagerepo::api: files_attrs FileSpec { key: Key { path: RepoPathBuf("B"), hgid: HgId("35e7525ce3a48913275d7061dd9a867ffef1e34d") }, attrs: FileAttributes { content: true, aux_data: false } }
  TRACE eagerepo::api:  found: 35e7525ce3a48913275d7061dd9a867ffef1e34d, 41 bytes
  AB (no-eol)

Clone (using edenapi clonedata, bypassing peer interface):

  $ cd $TESTTMP
  $ hg clone -U test:e1 --config remotefilelog.reponame=x cloned1
  Cloning x into $TESTTMP/cloned1
  DEBUG eagerepo::api: bookmarks master
  DEBUG eagerepo::api: commit_graph_segments 23d30dc6b70380b2d939023947578ae0e0198999 

Clone:

  $ cd $TESTTMP
  $ hg clone -U test:e1 cloned
  Cloning e1 into $TESTTMP/cloned
  DEBUG eagerepo::api: bookmarks master
  DEBUG eagerepo::api: commit_graph_segments 23d30dc6b70380b2d939023947578ae0e0198999 

  $ cd cloned

Commit hash and message are lazy

  $ LOG=dag::protocol=debug,eagerepo=debug hg log -T '{desc} {node}\n' -r 'all()'
  DEBUG dag::protocol: resolve ids [1] remotely
  DEBUG eagerepo::api: revlog_data 748104bd5058bf2c386d074d8dcf2704855380f6, 178c10ffbc2f92d5407c14478ae9d9dea81f232e, 23d30dc6b70380b2d939023947578ae0e0198999
  A 748104bd5058bf2c386d074d8dcf2704855380f6
  C 178c10ffbc2f92d5407c14478ae9d9dea81f232e
  D 23d30dc6b70380b2d939023947578ae0e0198999

Read file content:

  $ hg cat -r $C C
  DEBUG eagerepo::api: trees 0ccf968573574750913fcee533939cc7ebe7327d Some(TreeAttributes { manifest_blob: true, parents: true, child_metadata: false, augmented_trees: false })
  TRACE eagerepo::api:  found: 0ccf968573574750913fcee533939cc7ebe7327d, 170 bytes
  DEBUG eagerepo::api: files_attrs FileSpec { key: Key { path: RepoPathBuf("C"), hgid: HgId("a2e456504a5e61f763f1a0b36a6c247c7541b2b3") }, attrs: FileAttributes { content: true, aux_data: false } }
  TRACE eagerepo::api:  found: a2e456504a5e61f763f1a0b36a6c247c7541b2b3, 41 bytes
  C (no-eol)

Make a commit on tip, and amend. They do not trigger remote lookups:

  $ echo Z > Z
  $ LOG=error hg up -q tip
  $ LOG=dag::protocol=debug,dag::cache=trace hg commit -Am Z Z
  TRACE dag::cache: cached missing ae226a63078b2a472fa38ec61318bb37e8c10bfb (definitely missing)
  DEBUG dag::cache: reusing cache (1 missing)

  $ LOG=dag::protocol=debug,dag::cache=trace hg amend -m Z1
  TRACE dag::cache: cached missing 893a1eb784b46325fb3062573ba15a22780ebe4a (definitely missing)
  DEBUG dag::cache: reusing cache (1 missing)
  DEBUG dag::cache: reusing cache (1 missing)

Test that auto pull invalidates public() properly:

# Server: Prepare public (P20, master) and draft (D9) branches

    $ cd
    $ hg init server-autopull --config format.use-eager-repo=True
    $ drawdag --cwd server-autopull << 'EOS'
    >     D9
    >     :
    > P20 D1  # bookmark master = P20
    >  : /
    > P10
    >  :
    > P01
    > EOS

# Client: Fetch the initial master, using lazy changelog.

    $ cd
    $ newremoterepo
    $ setconfig paths.default=test:server-autopull
    $ hg debugchangelog --migrate lazy
    $ LOG= hg pull -q -B master

# Server: Move "master" forward P20 -> P99.

    $ drawdag --cwd ~/server-autopull << 'EOS'
    > P99  # bookmark master = P99
    >  :
    > P21
    >  |
    > desc(P20)
    > EOS

# Client: autopull D9 and move master forward, then calculate a revset
# containing "public()" should not require massive "resolve remotely" requests.
# There should be no "DEBUG dag::protocol: resolve ids (76) remotely" below.

    $ LOG=dag::protocol=debug hg log -r "only($D9,public())" -T '{desc}\n'
    DEBUG dag::protocol: resolve names [428b6ef7fec737262ee83ba89e4fab5e3a07db44] remotely
    pulling '428b6ef7fec737262ee83ba89e4fab5e3a07db44' from 'test:server-autopull'
    DEBUG dag::protocol: resolve names [a81a182e51718edfeccb2f62846c28c7b83de6f1] remotely
    DEBUG dag::protocol: resolve names [428b6ef7fec737262ee83ba89e4fab5e3a07db44] remotely
    D1
    D2
    D3
    D4
    D5
    D6
    D7
    D8
    D9
    >>> assert 'resolve ids (' not in _

Test that autopull does not make draft commits visible.

  $ hg log -r $D9 -T '{phase}\n'
  secret

Test that filtering revset does not use sequential fetches.

  $ cd
  $ hg init server-filtering-revset --config format.use-eager-repo=True
  $ drawdag --cwd ~/server-filtering-revset << 'EOS'
  > P01  # bookmark master = P01
  > EOS

  $ cd
  $ newremoterepo
  $ setconfig paths.default=test:server-filtering-revset
  $ hg debugchangelog --migrate lazy
  $ LOG= hg pull -q -B master

  $ drawdag --cwd ~/server-filtering-revset << 'EOS'
  > P30  # bookmark master = P30
  >  :
  > P01
  > EOS

  $ LOG= hg pull -q -B master

  $ LOG=dag::protocol=trace,eagerepo::api=debug hg log -r "reverse(master~20::master) & not(file(r're:.*'))"
  DEBUG dag::protocol: resolve ids [9] remotely
  DEBUG dag::protocol: resolve ids [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28] remotely
  DEBUG eagerepo::api: revlog_data * (glob)
  >>> assert _.count('revlog_data') == 1 and 0 <=  _.count('resolve id') < 3
