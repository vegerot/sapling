#modern-config-incompatible
#modern-config-incompatible

#require no-eden

  $ configure modern

  $ newserver master
  $ cat >> .hg/hgrc <<EOF
  > [remotefilelog]
  > cachelimit = 100B
  > manifestlimit = 100B
  > EOF
  $ hg debugdetectissues
  ran issue detector 'cachesizeexceedslimit', found 0 issues
  $ echo "a" > a ; hg add a ; hg commit -qAm a
  $ echo "b" > b ; hg add b ; hg commit -qAm b
  $ hg debugdetectissues
  ran issue detector 'cachesizeexceedslimit', found 0 issues
  $ cd ..
  $ clone master shallow --config remotenames.selectivepull=false
  $ cd shallow
  $ cat >> .hg/hgrc <<EOF
  > [remotefilelog]
  > cachelimit = 100B
  > manifestlimit = 100B
  > EOF
  $ hg debugdetectissues
  ran issue detector 'cachesizeexceedslimit', found 2 issues
  'cache_size_exceeds_limit': 'cache size of * exceeds configured limit of 100. 0 files skipped.' (glob)
  'manifest_size_exceeds_limit': 'manifest cache size of * exceeds configured limit of 100. 0 files skipped.' (glob)
