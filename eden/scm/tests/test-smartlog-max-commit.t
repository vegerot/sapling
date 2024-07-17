
#require no-eden


  $ configure modern
  $ enable smartlog rebase
  $ disable commitcloud

With max-commit-threshold and collapse-obsolete:

  $ newrepo
  $ drawdag << 'EOS'
  >   e5  E4  # amend: e1 -> E1
  > G :   :   # amend: e2 -> E2
  > | e1  E1  # amend: e3 -> E3
  > :/   /    # amend: e4 -> E4
  > E   E
  > |
  > :
  > | c5  C4  # amend: c1 -> C1
  > | :   :   # amend: c2 -> C2
  > | c1  C1  # amend: c3 -> C3
  > |/   /    # amend: c4 -> C4
  > C   C
  > :
  > A
  > EOS
  $ hg bookmark -r $G master
  $ hg sl -T '{desc}' --config smartlog.collapse-obsolete=true --config smartlog.max-commit-threshold=1
  smartlog: too many (25) commits, not rendering all of them
  (consider running 'hg doctor' to hide unrelated commits)
  o  G
  ╷
  ╷ o  C4
  ╭─╯
  ╷ o  c5
  ╭─╯
  ╷ o  E4
  ╭─╯
  ╷ o  e5
  ╭─╯
  o  C
  │
  ~

With a root commit:

  $ newrepo
  $ drawdag << 'EOS'
  >   E4
  >   :
  > G E1
  > :/
  > E C4
  > | :
  > | C1
  > :/
  > C
  > :
  > A
  > EOS
  $ hg bookmark -r $G master

  $ hg sl -T '{desc}' --config smartlog.max-commit-threshold=1 --config smartlog.collapse-obsolete=false
  smartlog: too many (15) commits, not rendering all of them
  (consider running 'hg doctor' to hide unrelated commits)
  o  G
  ╷
  ╷ o  C4
  ╭─╯
  ╷ o  E4
  ╭─╯
  o  C
  │
  ~

  $ hg sl -T '{desc}' --config smartlog.max-commit-threshold=1 --config smartlog.collapse-obsolete=true
  smartlog: too many (15) commits, not rendering all of them
  (consider running 'hg doctor' to hide unrelated commits)
  o  G
  ╷
  ╷ o  C4
  ╭─╯
  ╷ o  E4
  ╭─╯
  o  C
  │
  ~

