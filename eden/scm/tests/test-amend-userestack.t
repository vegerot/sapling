
#require no-eden


  $ eagerepo
Set up test environment.
  $ configure mutation
  $ enable amend rebase tweakdefaults
  $ mkcommit() {
  >   echo "$1" > "$1"
  >   hg add "$1"
  >   echo "add $1" > msg
  >   hg ci -l msg
  > }
  $ reset() {
  >   cd ..
  >   rm -rf userestack
  >   hg init userestack
  >   cd userestack
  > }
  $ showgraph() {
  >   hg log --graph -r '(::.)::' -T "{desc|firstline}" | sed \$d
  > }
  $ hg init userestack && cd userestack

Test that no preamend bookmark is created.
  $ mkcommit a
  $ mkcommit b
  $ mkcommit c
  $ mkcommit d
  $ hg up 7c3bad9141dcb46ff89abf5f61856facd56e476c
  0 files updated, 0 files merged, 2 files removed, 0 files unresolved
  $ hg amend -m "amended" --no-rebase
  hint[amend-restack]: descendants of 7c3bad9141dc are left behind - use 'hg restack' to rebase them
  hint[hint-ack]: use 'hg hint --ack amend-restack' to silence these hints
  $ hg book
  no bookmarks set

Test hg amend --fixup.
  $ showgraph
  @  amended
  │
  │ o  add d
  │ │
  │ o  add c
  │ │
  │ x  add b
  ├─╯
  o  add a

  $ hg amend --fixup
  warning: --fixup is deprecated and WILL BE REMOVED. use 'hg restack' instead.
  rebasing 4538525df7e2 "add c"
  rebasing 47d2a3944de8 "add d"
  $ showgraph
  o  add d
  │
  o  add c
  │
  @  amended
  │
  o  add a

Test that the operation field on the metadata is correctly set.
  $ hg debugmutation -r "all()"
   *  1f0dee641bb7258c56bd60e93edfa2405381c41e
  
   * amend by test at 1970-01-01T00:00:00 from: (glob)
      7c3bad9141dcb46ff89abf5f61856facd56e476c
  
   * rebase by test at 1970-01-01T00:00:00 from: (glob)
      4538525df7e2b9f09423636c61ef63a4cb872a2d
  
   * rebase by test at 1970-01-01T00:00:00 from: (glob)
      47d2a3944de8b013de3be9578e8e344ea2e6c097
  

Test hg amend --rebase
  $ hg amend -m "amended again" --rebase
  rebasing * "add c" (glob)
  rebasing * "add d" (glob)
  $ showgraph
  o  add d
  │
  o  add c
  │
  @  amended again
  │
  o  add a
