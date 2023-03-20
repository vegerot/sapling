#chg-compatible
#debugruntest-compatible

#testcases pythonstatus ruststatus
#if pythonstatus
  $ setconfig status.use-rust=false workingcopy.use-rust=false
#else
  $ setconfig status.use-rust=true workingcopy.use-rust=true
#endif

  $ configure modernclient

  $ enable sparse
  $ newclientrepo myrepo
  $ touch a
  $ hg commit -Aqm a
  $ hg rm a
  $ cat > .hg/sparse <<EOF
  > [exclude]
  > a
  > EOF

We should filter out "a" since it isn't included in the sparse profile.
  $ hg status
