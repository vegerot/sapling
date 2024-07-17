#chg-compatible
#debugruntest-incompatible


  $ eagerepo
  $ enable rebase
  $ setconfig phases.publish=false
  $ echo "[merge]" >> $HGRCPATH

  $ hg init repo
  $ cd repo
  $ echo a > a
  $ echo b > b
  $ hg commit -qAm ab
  $ echo c >> a
  $ echo c >> b
  $ hg commit -qAm c
  $ hg up -q ".^"
  $ echo d >> a
  $ echo d >> b
  $ hg commit -qAm d

Testing on-failure=continue
  $ echo on-failure=continue >> $HGRCPATH
  $ hg rebase -s 'desc(c)' -d 'desc(d)' --tool false
  rebasing 1f28a51c3c9b "c"
  merging a
  merging b
  merging a failed!
  merging b failed!
  unresolved conflicts (see hg resolve, then hg rebase --continue)
  [1]

  $ hg resolve --list
  U a
  U b

  $ hg rebase --abort
  rebase aborted

Testing on-failure=halt
  $ echo on-failure=halt >> $HGRCPATH
  $ hg rebase -s 'desc(c)' -d 'desc(d)' --tool false
  rebasing 1f28a51c3c9b "c"
  merging a
  merging b
  merging a failed!
  merge halted after failed merge (see hg resolve)
  [1]

  $ hg resolve --list
  U a
  U b

  $ hg rebase --abort
  rebase aborted

Testing on-failure=prompt
  $ cat <<EOS >> $HGRCPATH
  > [merge]
  > on-failure=prompt
  > [ui]
  > interactive=1
  > EOS
  $ cat <<EOS | hg rebase -s 'desc(c)' -d 'desc(d)' --tool false
  > y
  > n
  > EOS
  rebasing 1f28a51c3c9b "c"
  merging a
  merging b
  merging a failed!
  continue merge operation [(y)es/(n)o/(a)lways]? y
  merging b failed!
  continue merge operation [(y)es/(n)o/(a)lways]? n
  merge halted after failed merge (see hg resolve)
  [1]

  $ hg resolve --list
  U a
  U b

  $ hg rebase --abort
  rebase aborted

  $ cat <<EOS | hg rebase -s 'desc(c)' -d 'desc(d)' --tool false
  > a
  > EOS
  rebasing 1f28a51c3c9b "c"
  merging a
  merging b
  merging a failed!
  continue merge operation [(y)es/(n)o/(a)lways]? a
  merging b failed!
  unresolved conflicts (see hg resolve, then hg rebase --continue)
  [1]

  $ hg resolve --list
  U a
  U b

  $ hg rebase --abort
  rebase aborted

Check that successful tool with failed post-check halts the merge
  $ cat <<EOS >> $HGRCPATH
  > [merge-tools]
  > true.check=changed
  > EOS
  $ cat <<EOS | hg rebase -s 'desc(c)' -d 'desc(d)' --tool true
  > y
  > n
  > n
  > EOS
  rebasing 1f28a51c3c9b "c"
  merging a
  merging b
   output file a appears unchanged
  was merge successful (yn)? y
   output file b appears unchanged
  was merge successful (yn)? n
  merging b failed!
  continue merge operation [(y)es/(n)o/(a)lways]? n
  merge halted after failed merge (see hg resolve)
  [1]

  $ hg resolve --list
  R a
  U b

  $ hg rebase --abort
  rebase aborted

Check that conflicts with conflict check also halts the merge
  $ cat <<EOS >> $HGRCPATH
  > [merge-tools]
  > true.check=conflicts
  > true.premerge=keep
  > [merge]
  > on-failure=halt
  > EOS
  $ hg rebase -s 'desc(c)' -d 'desc(d)' --tool true
  rebasing 1f28a51c3c9b "c"
  merging a
  merging b
  merging a failed!
  merge halted after failed merge (see hg resolve)
  [1]

  $ hg resolve --list
  U a
  U b

  $ hg rebase --abort
  rebase aborted

Check that always-prompt also can halt the merge
  $ cat <<EOS | hg rebase -s 'desc(c)' -d 'desc(d)' --tool true --config merge-tools.true.check=prompt
  > y
  > n
  > EOS
  rebasing 1f28a51c3c9b "c"
  merging a
  merging b
  was merge of 'a' successful (yn)? y
  was merge of 'b' successful (yn)? n
  merging b failed!
  merge halted after failed merge (see hg resolve)
  [1]

  $ hg resolve --list
  R a
  U b

  $ hg rebase --abort
  rebase aborted

Check that successful tool otherwise allows the merge to continue
  $ hg rebase -s 'desc(c)' -d 'desc(d)' --tool echo --keep --config merge-tools.echo.premerge=keep
  rebasing 1f28a51c3c9b "c"
  merging a
  merging b
  $TESTTMP/repo/a *a~base* *a~other* (glob)
  $TESTTMP/repo/b *b~base* *b~other* (glob)
