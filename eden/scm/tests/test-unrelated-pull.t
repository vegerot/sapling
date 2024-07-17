#modern-config-incompatible

#require no-eden

#inprocess-hg-incompatible
  $ setconfig experimental.allowfilepeer=True

  $ hg init a
  $ cd a
  $ echo 123 > a
  $ hg add a
  $ hg commit -m "a" -u a

  $ cd ..
  $ hg init b
  $ cd b
  $ echo 321 > b
  $ hg add b
  $ hg commit -m "b" -u b

  $ hg pull ../a
  pulling from ../a
  searching for changes
  abort: repository is unrelated
  [255]

  $ hg pull -f ../a
  pulling from ../a
  searching for changes
  warning: repository is unrelated
  requesting all changes
  adding changesets
  adding manifests
  adding file changes

  $ hg heads
  commit:      9a79c33a9db3
  user:        a
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     a
  
  commit:      01f8062b2de5
  user:        b
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     b
  

  $ cd ..
