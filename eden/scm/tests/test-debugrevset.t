
#require no-eden

  $ eagerepo

Setup repo:
  $ newclientrepo
  $ drawdag << 'EOS'
  > G # bookmark master = G
  > |
  > F
  > |
  > E
  > |
  > D
  > |
  > C
  > |
  > B
  > |
  > A
  > EOS
  $ hg log -T "{node}\n"
  43195508e3bb704c08d24c40375bdd826789dd72
  a194cadd16930608adaa649035ad4c16930cbd0f
  9bc730a19041f9ec7cb33c626e811aa233efb18c
  f585351a92f85104bff7c284233c338b10eb1df7
  26805aba1e600a82e93661149f2313866a221a7b
  112478962961147124edd43549aedd1a335e44bf
  426bada5c67598ca65036d57d9e4b64b0c1ce7a0

Test hash prefix lookup:
  $ hg debugrevset 431
  43195508e3bb704c08d24c40375bdd826789dd72
  $ hg debugrevset 4
  abort: ambiguous identifier for '4': 426bada5c67598ca65036d57d9e4b64b0c1ce7a0, 43195508e3bb704c08d24c40375bdd826789dd72 available
  [255]
  $ hg debugrevset 6
  abort: unknown revision '6'
  [255]
  $ hg debugrevset thisshóuldnótbéfoünd
  abort: unknown revision 'thissh*' (glob)
  [255]

Test bookmark lookup
  $ hg book -r 'desc(C)' mybookmark
  $ hg debugrevset mybookmark
  26805aba1e600a82e93661149f2313866a221a7b

Test remote bookmark lookup
  $ hg debugrevset master
  43195508e3bb704c08d24c40375bdd826789dd72

Test dot revset lookup
  $ hg debugrevset .
  0000000000000000000000000000000000000000
  $ hg debugrevset ""
  0000000000000000000000000000000000000000
  $ hg up 43195508e3
  7 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ hg debugrevset .
  43195508e3bb704c08d24c40375bdd826789dd72
  $ hg debugrevset ""
  43195508e3bb704c08d24c40375bdd826789dd72

Test misc revsets
  $ hg debugrevset tip
  43195508e3bb704c08d24c40375bdd826789dd72
  $ hg debugrevset null
  0000000000000000000000000000000000000000

Test resolution priority
  $ hg book -r 'desc(A)' f
  $ hg debugrevset f
  426bada5c67598ca65036d57d9e4b64b0c1ce7a0
  $ hg debugrevset tip
  43195508e3bb704c08d24c40375bdd826789dd72
  $ hg debugrevset null
  0000000000000000000000000000000000000000


Test remote/lazy hash prefix lookup:
  $ newrepo remote
  $ drawdag << 'EOS'
  > C # bookmark master = C
  > |
  > B
  > |
  > A
  > EOS
  $ hg log -T "{node}\n"
  26805aba1e600a82e93661149f2313866a221a7b
  112478962961147124edd43549aedd1a335e44bf
  426bada5c67598ca65036d57d9e4b64b0c1ce7a0

  $ newclientrepo client test:remote

  $ hg debugrevset 426bada5c67598ca65036d57d9e4b64b0c1ce7a0
  426bada5c67598ca65036d57d9e4b64b0c1ce7a0

  $ hg debugrevset zzzbada5c67598ca65036d57d9e4b64b0c1ce7a0
  abort: unknown revision 'zzzbada5c67598ca65036d57d9e4b64b0c1ce7a0'
  [255]

  $ hg debugrevset 11
  112478962961147124edd43549aedd1a335e44bf


Fall back to changelog if "tip" isn't available in metalog:
  $ newclientrepo
  $ hg debugrevset tip
  0000000000000000000000000000000000000000
  $ drawdag << 'EOS'
  > A
  > EOS
  $ hg dbsh -c 'del ml["tip"]; ml.commit("no tip")'
  $ hg dbsh -c 'print(ml["tip"] or "no tip")'
  no tip
  $ hg debugrevset tip
  426bada5c67598ca65036d57d9e4b64b0c1ce7a0
Don't propagate a bad ml "tip":
  $ hg dbsh -c 'ml["tip"] = bin("112478962961147124edd43549aedd1a335e44bf"); ml.commit("bad tip")'
  $ hg debugrevset tip
  426bada5c67598ca65036d57d9e4b64b0c1ce7a0
