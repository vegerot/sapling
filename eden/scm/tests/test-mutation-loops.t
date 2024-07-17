
#require no-eden

  $ eagerepo
  $ enable amend rebase remotenames
  $ setconfig experimental.evolution=obsolete
  $ setconfig experimental.narrow-heads=true
  $ setconfig visibility.enabled=true
  $ setconfig mutation.record=true mutation.enabled=true

  $ newrepo
  $ echo "base" > base
  $ hg commit -Aqm base
  $ echo 1 > file
  $ hg commit -Aqm commit1
  $ for i in 2 3 4 5
  > do
  >   echo $i >> file
  >   hg amend -m "commit$i"
  > done
  $ hg debugmutation
   *  21c93100b04c543843a7dab4fa0d5bada061b7a0 amend by test at 1970-01-01T00:00:00 from:
      672a4910c364d425231d2dd2fb0486f32a2d88f4 amend by test at 1970-01-01T00:00:00 from:
      d3c8fd338cf40a496d981b2ada8df4108f575897 amend by test at 1970-01-01T00:00:00 from:
      932f02c9fad3fa46e55b62560c88eb67528b02f0 amend by test at 1970-01-01T00:00:00 from:
      e6c779c67aa947c951f334f4f312bd2b21d27e55
  

Loops are not normally possible, but they can sneak in through backfilling complex
obsmarker graphs.  Create a fake one to check behaviour.

  $ hg debugsh -c "with repo.lock(): s.mutation.recordentries(repo, [s.mutation.createsyntheticentry(repo, [s.node.bin(\"e6c779c67aa947c951f334f4f312bd2b21d27e55\"), s.node.bin(\"672a4910c364d425231d2dd2fb0486f32a2d88f4\")], s.node.bin(\"932f02c9fad3fa46e55b62560c88eb67528b02f0\"), \"loop\")], skipexisting=False)"
  $ tglogm --hidden
  @  21c93100b04c 'commit5'
  │
  │ x  672a4910c364 'commit4'  (Rewritten using amend into 21c93100b04c) (Rewritten using loop into 932f02c9fad3)
  ├─╯
  │ x  d3c8fd338cf4 'commit3'  (Rewritten using amend into 672a4910c364)
  ├─╯
  │ x  932f02c9fad3 'commit2'  (Rewritten using amend into d3c8fd338cf4)
  ├─╯
  │ x  e6c779c67aa9 'commit1'  (Rewritten using loop into 932f02c9fad3)
  ├─╯
  o  d20a80d4def3 'base'
  

  $ hg unhide e6c779c67aa9

Check the normal revsets.
  $ hg log -r 'predecessors(21c93100b04c)' -T '{node} {desc}\n'
  e6c779c67aa947c951f334f4f312bd2b21d27e55 commit1
  932f02c9fad3fa46e55b62560c88eb67528b02f0 commit2
  d3c8fd338cf40a496d981b2ada8df4108f575897 commit3
  672a4910c364d425231d2dd2fb0486f32a2d88f4 commit4
  21c93100b04c543843a7dab4fa0d5bada061b7a0 commit5
  $ hg log -r 'successors(e6c779c67aa9)' -T '{node} {desc}\n'
  e6c779c67aa947c951f334f4f312bd2b21d27e55 commit1
  21c93100b04c543843a7dab4fa0d5bada061b7a0 commit5

If successorssets doesn't handle loops, this next command will hang as it
continuously cycles round the commit2 to commit4 loop.

  $ tglogm
  @  21c93100b04c 'commit5'
  │
  │ x  e6c779c67aa9 'commit1'  (Rewritten using rewrite into 21c93100b04c)
  ├─╯
  o  d20a80d4def3 'base'
  
Similarly, check that predecessorsset is also safe.

  $ hg debugsh -c "ui.write(str([s.node.hex(n) for n in s.mutation.predecessorsset(repo, s.node.bin(\"21c93100b04c543843a7dab4fa0d5bada061b7a0\"))]) + '\n')"
  ['e6c779c67aa947c951f334f4f312bd2b21d27e55']
