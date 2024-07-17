  $ enable rebase undo

  $ setconfig commit.file-size-limit=5
  $ setconfig devel.hard-file-size-limit=10

  $ newclientrepo
  $ echo abc > foo
  $ hg add foo
  $ hg commit -m foo

  $ echo toobig > foo

  $ hg commit -m toobig
  abort: foo: size of 7 bytes exceeds maximum size of 5 bytes!
  (use '--config commit.file-size-limit=N' to override)
  [255]

  $ hg commit -m toobig --config "ui.supportcontact=Source Control"
  abort: foo: size of 7 bytes exceeds maximum size of 5 bytes!
  (contact Source Control for help or use '--config commit.file-size-limit=N' to override)
  [255]

  $ hg commit -m foo --config commit.file-size-limit=1KB

Above hard limit:
  $ echo reallyhumongous > foo

  $ hg commit -m foo --config commit.file-size-limit=1KB
  abort: foo: size of 16 bytes exceeds maximum size of 10 bytes!
  [255]

  $ hg commit -m toobig --config commit.file-size-limit=1KB --config "ui.supportcontact=Source Control"
  abort: foo: size of 16 bytes exceeds maximum size of 10 bytes!
  (contact Source Control for help)
  [255]

Can still override:

  $ hg commit -m toobig --config commit.file-size-limit=1KB --config devel.hard-file-size-limit=1KB


Rebasing shouldn't require re-overriding:

  $ newclientrepo
  $ drawdag <<EOS
  > B
  > |
  > A
  > EOS
  $ hg go -q $A
  $ echo toobig > foo
  $ hg commit -Aqm foo --config commit.file-size-limit=1KB
  $ hg rebase -d $B --config rebase.experimental.inmemory=true
  rebasing 802aace8cbe9 "foo"

  $ hg undo -q

  $ hg rebase -d $B --config rebase.experimental.inmemory=false
  rebasing 802aace8cbe9 "foo"
