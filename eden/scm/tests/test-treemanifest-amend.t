
#require no-eden


Crash in histpack code path where the amend destination already exists


  $ configure mutation-norecord
  $ enable undo treemanifest remotefilelog
  $ setconfig treemanifest.treeonly=1 remotefilelog.reponame=foo remotefilelog.cachepath=$TESTTMP/cache
  $ setconfig remotefilelog.write-hgcache-to-indexedlog=False remotefilelog.write-local-to-indexedlog=False
  $ newclientrepo
  $ drawdag << 'EOS'
  > B
  > |
  > A
  > EOS

  $ enable undo
  $ hg up -q $B
  $ echo foo > msg
  $ hg --config dummy.option=dummy commit --amend -l msg
  $ hg undo -q
  hint[undo-uncommit-unamend]: undoing amends discards their changes.
  to restore the changes to the working copy, run 'hg revert -r 220f69710758 --all'
  in the future, you can use 'hg unamend' instead of 'hg undo' to keep changes
  hint[hint-ack]: use 'hg hint --ack undo-uncommit-unamend' to silence these hints
  $ hg commit --amend -l msg

Make sure no invalid manifests were written:

  $ cd .hg/store/packs/manifests
  $ for i in *.histidx; do hg debughistorypack $i; done
  
  
  Node          P1 Node       P2 Node       Link Node     Copy From
  eb7988638387  41b34f08c135  000000000000  220f69710758  
  
  
  Node          P1 Node       P2 Node       Link Node     Copy From
  eb7988638387  41b34f08c135  000000000000  112478962961  
  41b34f08c135  000000000000  000000000000  426bada5c675  
