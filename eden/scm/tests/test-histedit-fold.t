(debugruntest fails under buck for some reason)
#chg-compatible

Test histedit extension: Fold commands
======================================

This test file is dedicated to testing the fold command in non conflicting
case.

Initialization
---------------

  $ eagerepo
  $ . "$TESTDIR/histedit-helpers.sh"

  $ enable histedit
  $ readconfig <<EOF
  > [alias]
  > logt = log --template '{node|short} {desc|firstline}\n'
  > EOF


Simple folding
--------------------
  $ addwithdate ()
  > {
  >     echo $1 > $1
  >     hg add $1
  >     hg ci -m $1 -d "$2 0"
  > }

  $ initrepo ()
  > {
  >     hg init r
  >     cd r
  >     addwithdate a 1
  >     addwithdate b 2
  >     addwithdate c 3
  >     addwithdate d 4
  >     addwithdate e 5
  >     addwithdate f 6
  > }

  $ initrepo

log before edit
  $ hg logt --graph
  @  178e35e0ce73 f
  │
  o  1ddb6c90f2ee e
  │
  o  532247a8969b d
  │
  o  ff2c9fa2018b c
  │
  o  97d72e5f12c7 b
  │
  o  8580ff50825a a
  

  $ hg histedit ff2c9fa2018b --commands - <<EOF | fixbundle
  > pick 1ddb6c90f2ee e
  > pick 178e35e0ce73 f
  > fold ff2c9fa2018b c
  > pick 532247a8969b d
  > EOF

log after edit
  $ hg logt --graph
  @  c4d7f3def76d d
  │
  o  575228819b7e f
  │
  o  505a591af19e e
  │
  o  97d72e5f12c7 b
  │
  o  8580ff50825a a
  

post-fold manifest
  $ hg manifest
  a
  b
  c
  d
  e
  f


check histedit_source, including that it uses the later date, from the first changeset

  $ hg log --debug --rev 'max(desc(f))'
  commit:      575228819b7e6ed69e8c0a6a383ee59a80db7358
  phase:       draft
  manifest:    81eede616954057198ead0b2c73b41d1f392829a
  user:        test
  date:        Thu Jan 01 00:00:06 1970 +0000
  files+:      c f
  extra:       branch=default
  extra:       histedit_source=7cad1d7030207872dfd1c3a7cb430f24f2884086,ff2c9fa2018b15fa74b33363bda9527323e2a99f
  description:
  f
  ***
  c
  
  

rollup will fold without preserving the folded commit's message or date

  $ OLDHGEDITOR=$HGEDITOR
  $ HGEDITOR=false
  $ hg histedit 97d72e5f12c7 --commands - <<EOF | fixbundle
  > pick 97d72e5f12c7 b
  > roll 505a591af19e e
  > pick 575228819b7e f
  > pick c4d7f3def76d d
  > EOF

  $ HGEDITOR=$OLDHGEDITOR

log after edit
  $ hg logt --graph
  @  bab801520cec d
  │
  o  58c8f2bfc151 f
  │
  o  5d939c56c72e b
  │
  o  8580ff50825a a
  

description is taken from rollup target commit

  $ hg log --debug --rev 'max(desc(b))'
  commit:      5d939c56c72e77e29f5167696218e2131a40f5cf
  phase:       draft
  manifest:    b5e112a3a8354e269b1524729f0918662d847c38
  user:        test
  date:        Thu Jan 01 00:00:02 1970 +0000
  files+:      b e
  extra:       branch=default
  extra:       histedit_source=97d72e5f12c7e84f85064aa72e5a297142c36ed9,505a591af19eed18f560af827b9e03d2076773dc
  description:
  b
  
  

check saving last-message.txt

  $ cat > $TESTTMP/abortfolding.py <<EOF
  > from sapling import util
  > def abortfolding(ui, repo, hooktype, **kwargs):
  >     ctx = repo[kwargs.get('node')]
  >     if set(ctx.files()) == {'c', 'd', 'f'}:
  >         return True # abort folding commit only
  >     ui.warn('allow non-folding commit\\n')
  > EOF
  $ cat > .hg/hgrc <<EOF
  > [hooks]
  > pretxncommit.abortfolding = python:$TESTTMP/abortfolding.py:abortfolding
  > EOF

  $ cat > $TESTTMP/editor.sh << EOF
  > echo "==== before editing"
  > cat \$1
  > echo "===="
  > echo "check saving last-message.txt" >> \$1
  > EOF

  $ rm -f .hg/last-message.txt
  $ hg status --rev '58c8f2bfc151^1::bab801520cec'
  A c
  A d
  A f
  $ HGEDITOR="sh $TESTTMP/editor.sh" hg histedit 58c8f2bfc151 --commands - <<EOF
  > pick 58c8f2bfc151 f
  > fold bab801520cec d
  > EOF
  ==== before editing
  f
  ***
  c
  ***
  d
  
  
  
  HG: Enter commit message.  Lines beginning with 'HG:' are removed.
  HG: Leave message empty to abort commit.
  HG: --
  HG: user: test
  HG: branch 'default'
  HG: added c
  HG: added d
  HG: added f
  ====
  abort: pretxncommit.abortfolding hook failed
  [255]

  $ cat .hg/last-message.txt
  f
  ***
  c
  ***
  d
  
  
  
  check saving last-message.txt

  $ cd ..
  $ rm -r r

folding preserves initial author but uses later date
----------------------------------------------------

  $ initrepo

  $ hg ci -d '7 0' --user "someone else" --amend --quiet

tip before edit
  $ hg log --rev .
  commit:      10c36dd37515
  user:        someone else
  date:        Thu Jan 01 00:00:07 1970 +0000
  summary:     f
  

  $ hg --config progress.debug=1 --debug \
  > histedit 1ddb6c90f2ee --commands - 2>&1 <<EOF | \
  > grep -E 'editing|unresolved'
  > pick 1ddb6c90f2ee e
  > fold 10c36dd37515 f
  > EOF
  progress: editing: pick 1ddb6c90f2ee e 1/2 changes (50.00%)
  progress: editing: fold 10c36dd37515 f 2/2 changes (100.00%)
  progress: editing (end)

tip after edit, which should use the later date, from the second changeset
  $ hg log --rev .
  commit:      e4f3ec5d0b40
  user:        test
  date:        Thu Jan 01 00:00:07 1970 +0000
  summary:     e
  

  $ cd ..
  $ rm -r r

folding and creating no new change doesn't break:
-------------------------------------------------

folded content is dropped during a merge. The folded commit should properly disappear.

  $ mkdir fold-to-empty-test
  $ cd fold-to-empty-test
  $ hg init
  $ printf "1\n2\n3\n" > file
  $ hg add file
  $ hg commit -m '1+2+3'
  $ echo 4 >> file
  $ hg commit -m '+4'
  $ echo 5 >> file
  $ hg commit -m '+5'
  $ echo 6 >> file
  $ hg commit -m '+6'
  $ hg logt --graph
  @  251d831eeec5 +6
  │
  o  888f9082bf99 +5
  │
  o  617f94f13c0f +4
  │
  o  0189ba417d34 1+2+3
  

  $ hg histedit 617f94f13c0faff2ff307641901637b91cbd7c7b --commands - << EOF
  > pick 617f94f13c0f 1 +4
  > drop 888f9082bf99 2 +5
  > fold 251d831eeec5 3 +6
  > EOF
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  merging file
  warning: 1 conflicts while merging file! (edit, then use 'hg resolve --mark')
  Fix up the change (fold 251d831eeec5)
  (hg histedit --continue to resume)
  [1]
There were conflicts, we keep P1 content. This
should effectively drop the changes from +6.

  $ hg status -v
  M file
  ? file.orig
  # The repository is in an unfinished *histedit* state.
  
  # Unresolved merge conflicts:
  # 
  #     file
  # 
  # To mark files as resolved:  hg resolve --mark FILE
  
  # To continue:                hg histedit --continue
  # To abort:                   hg histedit --abort
  
  $ hg resolve -l
  U file
  $ hg revert -r 'p1()' file
  $ hg resolve --mark file
  (no more unresolved files)
  continue: hg histedit --continue
  $ hg histedit --continue
  251d831eeec5: empty changeset
  $ hg logt --graph
  @  617f94f13c0f +4
  │
  o  0189ba417d34 1+2+3
  

  $ cd ..


Test fold through dropped
-------------------------


Test corner case where folded revision is separated from its parent by a
dropped revision.


  $ hg init fold-with-dropped
  $ cd fold-with-dropped
  $ printf "1\n2\n3\n" > file
  $ hg commit -Am '1+2+3'
  adding file
  $ echo 4 >> file
  $ hg commit -m '+4'
  $ echo 5 >> file
  $ hg commit -m '+5'
  $ echo 6 >> file
  $ hg commit -m '+6'
  $ hg logt -G
  @  251d831eeec5 +6
  │
  o  888f9082bf99 +5
  │
  o  617f94f13c0f +4
  │
  o  0189ba417d34 1+2+3
  
  $ hg histedit 617f94f13c0faff2ff307641901637b91cbd7c7b --commands -  << EOF
  > pick 617f94f13c0f 1 +4
  > drop 888f9082bf99 2 +5
  > fold 251d831eeec5 3 +6
  > EOF
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  merging file
  warning: 1 conflicts while merging file! (edit, then use 'hg resolve --mark')
  Fix up the change (fold 251d831eeec5)
  (hg histedit --continue to resume)
  [1]
  $ cat > file << EOF
  > 1
  > 2
  > 3
  > 4
  > 5
  > EOF
  $ hg resolve --mark file
  (no more unresolved files)
  continue: hg histedit --continue
  $ hg commit -m '+5.2'
  $ echo 6 >> file
  $ HGEDITOR=cat hg histedit --continue
  +4
  ***
  +5.2
  ***
  +6
  
  
  
  HG: Enter commit message.  Lines beginning with 'HG:' are removed.
  HG: Leave message empty to abort commit.
  HG: --
  HG: user: test
  HG: branch 'default'
  HG: changed file
  $ hg logt -G
  @  10c647b2cdd5 +4
  │
  o  0189ba417d34 1+2+3
  
  $ hg export tip
  # HG changeset patch
  # User test
  # Date 0 0
  #      Thu Jan 01 00:00:00 1970 +0000
  # Node ID 10c647b2cdd54db0603ecb99b2ff5ce66d5a5323
  # Parent  0189ba417d34df9dda55f88b637dcae9917b5964
  +4
  ***
  +5.2
  ***
  +6
  
  diff -r 0189ba417d34 -r 10c647b2cdd5 file
  --- a/file	Thu Jan 01 00:00:00 1970 +0000
  +++ b/file	Thu Jan 01 00:00:00 1970 +0000
  @@ -1,3 +1,6 @@
   1
   2
   3
  +4
  +5
  +6
  $ cd ..


Folding with initial rename (issue3729)
---------------------------------------

  $ hg init fold-rename
  $ cd fold-rename
  $ echo a > a.txt
  $ hg add a.txt
  $ hg commit -m a
  $ hg rename a.txt b.txt
  $ hg commit -m rename
  $ echo b >> b.txt
  $ hg commit -m b

  $ hg logt --follow b.txt
  e0371e0426bc b
  1c4f440a8085 rename
  6c795aa153cb a

  $ hg histedit 1c4f440a8085 --commands - << EOF | fixbundle
  > pick 1c4f440a8085 rename
  > fold e0371e0426bc b
  > EOF

  $ hg logt --follow b.txt
  cf858d235c76 rename
  6c795aa153cb a

  $ cd ..

Folding with swapping
---------------------

This is an excuse to test hook with histedit temporary commit (issue4422)


  $ hg init issue4422
  $ cd issue4422
  $ echo a > a.txt
  $ hg add a.txt
  $ hg commit -m a
  $ echo b > b.txt
  $ hg add b.txt
  $ hg commit -m b
  $ echo c > c.txt
  $ hg add c.txt
  $ hg commit -m c

  $ hg logt
  a1a953ffb4b0 c
  199b6bb90248 b
  6c795aa153cb a

Setup the proper environment variable symbol for the platform, to be subbed
into the hook command.
#if windows
  $ NODE="%HG_NODE%"
#else
  $ NODE="\$HG_NODE"
#endif
  $ hg histedit 6c795aa153cb --config hooks.commit="echo commit $NODE" --commands - << EOF | fixbundle
  > pick 199b6bb90248 b
  > fold a1a953ffb4b0 c
  > pick 6c795aa153cb a
  > EOF
  commit 16b87e97178dde2af2f3c6f6ddda882292f21d13
  commit 9599899f62c05f4377548c32bf1c9f1a39634b0c

  $ hg logt
  9599899f62c0 a
  79b99e9c8e49 b

  $ echo "foo" > amended.txt
  $ hg add amended.txt
  $ hg ci -q --amend -I amended.txt

Test that folding multiple changes in a row doesn't show multiple
editors.

  $ echo foo >> foo
  $ hg add foo
  $ hg ci -m foo1
  $ echo foo >> foo
  $ hg ci -m foo2
  $ echo foo >> foo
  $ hg ci -m foo3
  $ hg logt
  21679ff7675c foo3
  b7389cc4d66e foo2
  0e01aeef5fa8 foo1
  578c7455730c a
  79b99e9c8e49 b
  $ cat > "$TESTTMP/editor.sh" <<EOF
  > echo ran editor >> "$TESTTMP/editorlog.txt"
  > cat \$1 >> "$TESTTMP/editorlog.txt"
  > echo END >> "$TESTTMP/editorlog.txt"
  > echo merged foos > \$1
  > EOF
  $ HGEDITOR="sh \"$TESTTMP/editor.sh\"" hg histedit 'max(desc(a))' --commands - <<EOF | fixbundle
  > pick 578c7455730c 1 a
  > pick 0e01aeef5fa8 2 foo1
  > fold b7389cc4d66e 3 foo2
  > fold 21679ff7675c 4 foo3
  > EOF
  $ hg logt
  e8bedbda72c1 merged foos
  578c7455730c a
  79b99e9c8e49 b
Editor should have run only once
  $ cat $TESTTMP/editorlog.txt
  ran editor
  foo1
  ***
  foo2
  ***
  foo3
  
  
  
  HG: Enter commit message.  Lines beginning with 'HG:' are removed.
  HG: Leave message empty to abort commit.
  HG: --
  HG: user: test
  HG: branch 'default'
  HG: added foo
  END

  $ cd ..

Test rolling into a commit with multiple children (issue5498)

  $ hg init roll
  $ cd roll
  $ echo a > a
  $ hg commit -qAm aa
  $ echo b > b
  $ hg commit -qAm bb
  $ hg up -q ".^"
  $ echo c > c
  $ hg commit -qAm cc
  $ hg log -G -T '{node|short} {desc}'
  @  5db65b93a12b cc
  │
  │ o  301d76bdc3ae bb
  ├─╯
  o  8f0162e483d0 aa
  

  $ hg histedit . --commands - << EOF
  > r 5db65b93a12b
  > EOF
  hg: parse error: first changeset cannot use verb "roll"
  [255]
  $ hg log -G -T '{node|short} {desc}'
  @  5db65b93a12b cc
  │
  │ o  301d76bdc3ae bb
  ├─╯
  o  8f0162e483d0 aa
  

  $ cd ..

Fold/roll shouldn't trigger a merge:

  $ hg init rollmerge
  $ cd rollmerge
  $ echo a > a
  $ hg commit -qAm a
  $ echo b > a
  $ hg commit -qAm b
  $ hg log -G -T '{node|short} {desc}'
  @  1e6c11564562 b
  │
  o  cb9a9f314b8b a
  
Set a bogus mergedriver as a tripwire to make sure we don't invoke merge driver.
  $ hg histedit --config extensions.mergedriver= --config experimental.mergedriver=dontrunthis --commands - << EOF
  > p cb9a9f314b8b
  > r 1e6c11564562
  > EOF
  $ hg log -G -T '{node|short} {desc}'
  @  9e233947f73d a
  
