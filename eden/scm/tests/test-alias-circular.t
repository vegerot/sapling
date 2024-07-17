
#require no-eden


  $ eagerepo
Alias can override builtin commands.

  $ newrepo
  $ setconfig alias.log="log -T 'x\n'"
  $ hg log -r null
  x

Alias can override a builtin command to another builtin command.

  $ newrepo
  $ setconfig alias.log=id
  $ hg log -r null
  000000000000

Alias can refer to another alias. Order does not matter.

  $ newrepo
  $ cat >> .hg/hgrc <<EOF
  > [alias]
  > a = b
  > b = log -r null -T 'x\n'
  > c = b
  > EOF
  $ hg a
  x
  $ hg c
  x

Alias cannot form a cycle.

  $ newrepo
  $ cat >> .hg/hgrc << EOF
  > [alias]
  > c = a
  > a = b
  > b = c
  > logwithsuffix = logwithsuff
  > log = log
  > EOF

  $ hg a
  abort: circular alias: a
  [255]
  $ hg b
  abort: circular alias: b
  [255]
  $ hg c
  abort: circular alias: c
  [255]
  $ hg log -r null -T 'x\n'
  x

Prefix matching is disabled in aliases

  $ hg logwithsuffix
  unknown command 'logwithsuff'
  (use 'hg help' to get help)
  [255]
