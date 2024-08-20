
  $ setconfig checkout.use-rust=true
  $ setconfig experimental.nativecheckout=true

  $ newclientrepo
  $ drawdag <<'EOS'
  > A   # A/foo = foo
  >     # A/bar = bar
  > EOS

#if eden

Quick check for making sure this test is capable of using EdenFS
  $ ls -a $TESTTMP/.eden-backing-repos
  repo1

#endif


Unknown file w/ different content - conflict:
  $ echo nope > foo
  $ hg go $A
  abort: 1 conflicting file changes:
   foo
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]

Checking out to diff without file where file removed locally
  $ newclientrepo
  $ drawdag <<EOS
  > B  # B/file = foo
  > |
  > A
  > EOS
  $ hg go $B -qC
  $ hg rm file
  $ hg go $A
  abort: 1 conflicting file changes:
   file
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]

Respect merge marker file:
  $ newclientrepo
  $ drawdag <<'EOS'
  > B   # B/foo = changed
  > |
  > A   # A/foo = foo
  > EOS

  $ hg go -qC $A
  $ echo diverged > foo
  $ hg go -q --merge $B
  warning: 1 conflicts while merging foo! (edit, then use 'hg resolve --mark')
  [1]

  $ hg go $B
  abort: outstanding merge conflicts
  (use 'hg resolve --list' to list, 'hg resolve --mark FILE' to mark resolved)
  [255]

Run it again to make sure we didn't clear out state file:
  $ hg go $B
  abort: outstanding merge conflicts
  (use 'hg resolve --list' to list, 'hg resolve --mark FILE' to mark resolved)
  [255]

  $ hg go --continue
  abort: outstanding merge conflicts
  (use 'hg resolve --list' to list, 'hg resolve --mark FILE' to mark resolved)
  [255]

  $ hg resolve --mark foo
  (no more unresolved files)
  continue: hg goto --continue

  $ hg go --continue
  $ hg st
  M foo
  ? foo.orig


Can continue interrupted checkout:
  $ newclientrepo
  $ drawdag <<'EOS'
  > A   # A/foo = foo
  >     # A/bar = bar
  > EOS

  $ hg go -q null
  $ FAILPOINTS=checkout-post-progress=return hg go $A
  abort: checkout errors:
   Error set by checkout-post-progress FAILPOINTS
  [255]

  $ hg whereami
  0000000000000000000000000000000000000000

  $ hg go --continue $A --rev $A
  abort: can't specify a destination commit and --continue
  [255]

  $ LOG=checkout=debug hg go -q --continue 2>&1 | grep skipped_count
  DEBUG checkout:apply_store: checkout: skipped files based on progress skipped_count=3 (no-eden !)
  DEBUG checkout:apply_store: checkout: skipped files based on progress skipped_count=0 (eden !)
  $ hg st
  $ tglog
  @  a19fc4bcafed 'A'

  $ hg go --continue
  abort: not in an interrupted goto state
  [255]


Don't fail with open files that can't be deleted:
  $ newclientrepo unlink_fail
  $ drawdag <<'EOS'
  > B   # B/foo = (removed)
  > |   # B/bar = different
  > |
  > A   # A/foo = foo
  >     # A/bar = bar
  > EOS


  $ hg go -q $A

    with open("unlink_fail/foo"), open("unlink_fail/bar"):

      $ hg go $B
      update failed to remove foo: can't remove file "*foo": The process cannot access the file because it is being used by another process. (os error 32)! (glob) (windows !) (no-eden !)
      2 files updated, 0 files merged, 1 files removed, 0 files unresolved


Respect other repo states:
  $ newclientrepo
  $ drawdag <<'EOS'
  > B   # B/foo = two
  > 
  > A   # A/foo = one
  > EOS

  $ hg go -q $A
  $ hg graft -r $B
  grafting e57212eac5db "B"
  merging foo
  warning: 1 conflicts while merging foo! (edit, then use 'hg resolve --mark')
  abort: unresolved conflicts, can't continue
  (use 'hg resolve' and 'hg graft --continue')
  [255]
  
  $ hg go $B
  abort: graft in progress
  (use 'hg graft --continue' to continue or
       'hg graft --abort' to abort)
  [255]

Various invalid arg combos:

  $ newclientrepo
  $ hg go foo --rev bar
  abort: goto requires exactly one destination commit but got: ["foo", "bar"]
  [255]

  $ hg go
  abort: you must specify a destination to update to, for example "hg goto main".
  [255]

--clean overwrites conflicts:
  $ newclientrepo
  $ drawdag <<'EOS'
  > A   # A/foo = foo
  >     # A/bar = bar
  > B   # B/foo = baz
  > EOS
  $ hg go -q $B
  $ echo diverged > foo
  $ hg st
  M foo
  $ hg go $A
  abort: 1 conflicting file changes:
   foo
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]

  $ echo untracked > bar
  $ hg rm B
  $ hg st
  M foo
  R B
  ? bar
  $ hg go $A
  abort: * conflicting file changes: (glob)
   B
   bar
   foo
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]

  $ hg go -q --clean $A
  $ hg st
  $ cat foo
  foo (no-eol)
  $ cat bar
  bar (no-eol)
  $ cat B
  cat: B: $ENOENT$
  [1]

--clean gets you out of merge state:
  $ newclientrepo
  $ drawdag <<'EOS'
  > B   # B/foo = two
  > |
  > A   # A/foo = one
  > EOS
  $ hg go -q $A
  $ echo diverged > foo
  $ hg go --merge $B
  merging foo
  warning: 1 conflicts while merging foo! (edit, then use 'hg resolve --mark')
  1 files updated, 0 files merged, 0 files removed, 1 files unresolved
  use 'hg resolve' to retry unresolved file merges
  [1]
  $ hg go -qC $B
  $ hg st
  ? foo.orig
  $ cat foo
  two (no-eol)

--clean doesn't delete added files:
  $ newclientrepo
  $ touch a b c d
  $ hg commit -Aqm foo
  $ touch foo
  $ hg add foo
  $ rm a
  $ hg rm b
  $ echo c > c
  $ hg st
  M c
  A foo
  R b
  ! a
  $ hg go -qC .
  $ hg st
  ? foo

Non --clean keeps unconflicting changes:
  $ newclientrepo
  $ drawdag <<'EOS'
  > B
  > |
  > A
  > EOS
  $ hg go -q $A
  $ echo foo >> A
  $ touch foo
  $ mkdir bar
  $ echo bar > bar/bar
  $ hg st
  M A
  ? bar/bar
  ? foo
  $ hg go -q $B
  $ hg st
  M A
  ? bar/bar
  ? foo

Update active bookmark
  $ newclientrepo
  $ drawdag <<'EOS'
  > B  # bookmark BOOK_B = B
  > |
  > A  # bookmark BOOK_A = A
  > EOS
  $ hg go -q BOOK_A --inactive
  $ cat .hg/bookmarks.current
  cat: .hg/bookmarks.current: $ENOENT$
  [1]
  $ hg go BOOK_B
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  (activating bookmark BOOK_B)
  $ cat .hg/bookmarks.current
  BOOK_B (no-eol)
  $ hg go BOOK_A
  0 files updated, 0 files merged, 1 files removed, 0 files unresolved
  (changing active bookmark from BOOK_B to BOOK_A)
  $ cat .hg/bookmarks.current
  BOOK_A (no-eol)
  $ hg go $B
  1 files updated, 0 files merged, 0 files removed, 0 files unresolved
  (leaving bookmark BOOK_A)
  $ cat .hg/bookmarks.current
  cat: .hg/bookmarks.current: $ENOENT$
  [1]

#if no-windows
Support "update" and "goto" hooks:
  $ newclientrepo
  $ hg go -q . --config 'hooks.pre-update=echo update' --config 'hooks.pre-goto=echo goto'
  goto
  update
#endif

#if no-windows
Support "preupdate" and "update" hooks:
  $ newclientrepo
  $ drawdag <<'EOS'
  > A
  > EOS
  $ setconfig 'hooks.preupdate=echo PRE PARENT1: $HG_PARENT1 && echo PRE PARENT2: $HG_PARENT2; exit 1'
  $ setconfig 'hooks.update=echo POST PARENT1: $HG_PARENT1 && echo POST PARENT2: $HG_PARENT2 && echo POST ERROR: $HG_ERROR'
  $ hg go -q $A
  PRE PARENT1: 0000000000000000000000000000000000000000
  PRE PARENT2: 426bada5c67598ca65036d57d9e4b64b0c1ce7a0
  abort: preupdate hook exited with status 1
  [255]
  $ hg whereami
  0000000000000000000000000000000000000000
  $ setconfig 'hooks.preupdate=echo PARENT1: $HG_PARENT1 && echo PARENT2: $HG_PARENT2; exit 0'
  $ hg go -q $A
  PARENT1: 0000000000000000000000000000000000000000
  PARENT2: 426bada5c67598ca65036d57d9e4b64b0c1ce7a0
  POST PARENT1: 426bada5c67598ca65036d57d9e4b64b0c1ce7a0
  POST PARENT2:
  POST ERROR: 0
  $ hg whereami
  426bada5c67598ca65036d57d9e4b64b0c1ce7a0
#endif

Test --check
  $ newclientrepo
  $ drawdag <<'EOS'
  > A
  > EOS
  $ touch B
  $ hg go --check -q $A
  $ hg st
  ? B
  $ rm A
  $ SL_LOG=checkout_info=debug hg go --check -q null
  DEBUG checkout_info: checkout_mode="rust"
  abort: uncommitted changes
  [255]
  $ hg go --clean --check -q null
  abort: can only specify one of -C/--clean, -c/--check, or -m/--merge
  [255]

Bail on dir/path conflict with added file:
  $ newclientrepo
  $ drawdag <<'EOS'
  > B  # B/dir/foo=foo
  > |
  > A
  > EOS
  $ hg go -q $A
  $ touch dir
  $ hg add dir
TODO(sggutier): In this case EdenFS and non-EdenFS behavior differ, fix this later
  $ hg go $B
  abort: 1 conflicting file changes: (no-eden !)
   dir (no-eden !)
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them) (no-eden !)
  abort: file metadata for dir not found at source commit (eden !)
  [255]

Bail on untracked file conflict only if contents differ:
  $ newclientrepo
  $ drawdag <<'EOS'
  > B  # B/foo=foo\n
  > |
  > A
  > EOS
  $ hg go -q $A
  $ echo bar > foo
  $ hg go $B
  abort: 1 conflicting file changes:
   foo
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]
  $ echo foo > foo
  $ hg go -q $B

Bail on untracked file path conflict:
  $ newclientrepo
  $ drawdag <<'EOS'
  > B  # B/foo/bar=foo\n
  > |
  > A
  > EOS
  $ hg go -q $A
  $ echo foo > foo
TODO(sggutier): In this case EdenFS and non-EdenFS behavior differ, fix this later
  $ hg go $B
  abort: 1 conflicting file changes: (no-eden !)
   foo (no-eden !)
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them) (no-eden !)
  abort: file metadata for foo not found at source commit (eden !)
  [255]
  $ rm foo
  $ mkdir -p foo/bar
  $ echo foo > foo/bar/baz
TODO(sggutier): In this case EdenFS and non-EdenFS behavior differ, fix this later
  $ hg go $B
  abort: 1 conflicting file changes: (no-eden !)
   foo/bar/baz (no-eden !)
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them) (no-eden !)
  [255] (no-eden !)
  update complete (eden !)
  $ hg go -q $B --config experimental.checkout.rust-path-conflicts=false
  $ hg st
  ! foo/bar (eden !)
  ? foo/bar/baz (eden !)

Deleted file replaced by untracked directory:
  $ newclientrepo
  $ drawdag <<'EOS'
  > B  # B/foo=bar\n
  > |
  > A  # A/foo=foo\n
  > EOS
  $ hg go -q $A
  $ rm foo
  $ mkdir foo
  $ echo foo > foo/bar
  $ hg st
  ! foo
  ? foo/bar
  $ hg go $B
  abort: 1 conflicting file changes:
   foo
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]
  $ hg rm foo --mark
  $ hg add foo/bar
  $ hg st
  A foo/bar
  R foo
  $ hg go $B
  abort: 1 conflicting file changes:
   foo
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]
TODO(sggutier): This is yet another case of differing behavior between Eden and non-Eden
  $ hg go -qC $B
  $ hg st
  ! foo (eden !)
  ? foo/bar (eden !)

#if no-eden
Don't output too many conflicts. This behavior only occurs on non-EdenFS (no need to fix):
  $ newclientrepo
  $ drawdag <<'EOS'
  > B  # B/foo=bar\n
  > |
  > A
  > EOS
  $ hg go -q $A
  $ mkdir foo
  $ for i in `seq 100`; do
  >   touch foo/file$i
  > done
  $ hg go -q $B
  abort: 100 conflicting file changes:
   foo/file* (glob)
   foo/file* (glob)
   foo/file* (glob)
   foo/file* (glob)
   foo/file* (glob)
   ...and 95 more
  (commit, shelve, goto --clean to discard all your changes, or goto --merge to merge them)
  [255]
#endif

Test update_distance logging:
  $ newclientrepo
  $ drawdag <<'EOS'
  > C
  > |
  > B D
  > |/
  > A
  > EOS
  $ LOG=update_size=trace hg go -q $A
   INFO update_size: update_distance=1
  $ LOG=update_size=trace hg go -q $A
   INFO update_size: update_distance=0
  $ LOG=update_size=trace hg go -q $D
   INFO update_size: update_distance=1
  $ LOG=update_size=trace hg go -q $C
   INFO update_size: update_distance=3
  $ LOG=update_size=trace hg go -q $B
   INFO update_size: update_distance=1
  $ LOG=update_size=trace hg go -q null
   INFO update_size: update_distance=2

#if unix-permissions no-eden
# Test output when there are lots of filesystem errors:

  $ newclientrepo
  $ mkdir dir
  $ for i in `seq 10`; do touch dir/file_$i; done
  $ hg commit -Aqm foo
  $ hg go -q null
  $ mkdir dir
  $ chmod 444 dir
  $ hg go tip
  abort: error writing files:
   dir/file_1: can't clear conflicts after handling error "Permission denied (os error 13)": can't lstat "$TESTTMP/*/dir/file_1": Permission denied (os error 13) (glob)
   dir/file_10: can't clear conflicts after handling error "Permission denied (os error 13)": can't lstat "$TESTTMP/*/dir/file_10": Permission denied (os error 13) (glob)
   dir/file_2: can't clear conflicts after handling error "Permission denied (os error 13)": can't lstat "$TESTTMP/*/dir/file_2": Permission denied (os error 13) (glob)
   dir/file_3: can't clear conflicts after handling error "Permission denied (os error 13)": can't lstat "$TESTTMP/*/dir/file_3": Permission denied (os error 13) (glob)
   dir/file_4: can't clear conflicts after handling error "Permission denied (os error 13)": can't lstat "$TESTTMP/*/dir/file_4": Permission denied (os error 13) (glob)
   ...and 5 more
  [255]
#endif

# Test output when there are lots of edenapi errors:
#if no-eden

  $ newclientrepo broken_client test:broken_server
  $ cd ~/broken_server
  $ for i in `seq 10`; do touch file_$i; done
  $ hg commit -Aqm foo
  $ hg book master
  $ cd ~/broken_client
  $ hg pull -q
  $ FAILPOINTS=eagerepo::api::files_attrs=return hg go master
  abort: error fetching files:
   b80de5d138758541c5f05265ad144ab9fa86d1db file_1: Network Error: server responded 500 Internal Server Error for eager://$TESTTMP/broken_server/files_attrs: failpoint. Headers: {}
   b80de5d138758541c5f05265ad144ab9fa86d1db file_10: Network Error: server responded 500 Internal Server Error for eager://$TESTTMP/broken_server/files_attrs: failpoint. Headers: {}
   b80de5d138758541c5f05265ad144ab9fa86d1db file_2: Network Error: server responded 500 Internal Server Error for eager://$TESTTMP/broken_server/files_attrs: failpoint. Headers: {}
   b80de5d138758541c5f05265ad144ab9fa86d1db file_3: Network Error: server responded 500 Internal Server Error for eager://$TESTTMP/broken_server/files_attrs: failpoint. Headers: {}
   b80de5d138758541c5f05265ad144ab9fa86d1db file_4: Network Error: server responded 500 Internal Server Error for eager://$TESTTMP/broken_server/files_attrs: failpoint. Headers: {}
   ...and 5 more
  [255]

#endif
