#modern-config-incompatible

#require no-eden

  $ setconfig experimental.allowfilepeer=True

  $ hg init a
  $ hg clone a b
  updating to branch default
  0 files updated, 0 files merged, 0 files removed, 0 files unresolved
  $ cd a

With no paths:

  $ hg paths
  $ hg paths unknown
  not found!
  [1]
  $ hg paths -Tjson
  [
  ]

With paths:

  $ echo '[paths]' >> .hg/hgrc
  $ echo 'dupe = ../b#tip' >> .hg/hgrc
  $ echo 'expand = $SOMETHING/bar' >> .hg/hgrc
  $ cd ..
  $ cd a
  $ hg paths
  dupe = $TESTTMP/b#tip
  expand = $TESTTMP/a/$SOMETHING/bar
  $ SOMETHING=foo hg paths
  dupe = $TESTTMP/b#tip
  expand = $TESTTMP/a/foo/bar
#if msys
  $ SOMETHING=//foo hg paths
  dupe = $TESTTMP/b#tip
  expand = \\foo\bar
#else
  $ SOMETHING=/foo hg paths
  dupe = $TESTTMP/b#tip
  expand = /foo/bar
#endif
  $ hg paths -q
  dupe
  expand
  $ hg paths dupe
  $TESTTMP/b#tip
  $ hg paths -q dupe
  $ hg paths unknown
  not found!
  [1]
  $ hg paths -q unknown
  [1]

formatter output with paths:

  $ echo 'dupe:pushurl = https://example.com/dupe' >> .hg/hgrc
  $ hg paths -Tjson | sed 's|\\\\|\\|g'
  [
   {
    "name": "dupe",
    "pushurl": "https://example.com/dupe",
    "url": "$TESTTMP/b#tip"
   },
   {
    "name": "expand",
    "url": "$TESTTMP/a/$SOMETHING/bar"
   }
  ]
  $ hg paths -Tjson dupe | sed 's|\\\\|\\|g'
  [
   {
    "name": "dupe",
    "pushurl": "https://example.com/dupe",
    "url": "$TESTTMP/b#tip"
   }
  ]
  $ hg paths -Tjson -q unknown
  [
  ]
  [1]

log template:

 (behaves as a {name: path-string} dict by default)

  $ hg log -rnull -T '{peerurls}\n'
  dupe=$TESTTMP/b#tip expand=$TESTTMP/a/$SOMETHING/bar
  $ hg log -rnull -T '{join(peerurls, "\n")}\n'
  dupe=$TESTTMP/b#tip
  expand=$TESTTMP/a/$SOMETHING/bar
  $ hg log -rnull -T '{peerurls % "{name}: {url}\n"}'
  dupe: $TESTTMP/b#tip
  expand: $TESTTMP/a/$SOMETHING/bar
  $ hg log -rnull -T '{get(peerurls, "dupe")}\n'
  $TESTTMP/b#tip

 (sub options can be populated by map/dot operation)

  $ hg log -rnull \
  > -T '{get(peerurls, "dupe") % "url: {url}\npushurl: {pushurl}\n"}'
  url: $TESTTMP/b#tip
  pushurl: https://example.com/dupe
  $ hg log -rnull -T '{peerurls.dupe.pushurl}\n'
  https://example.com/dupe

 (in JSON, it's a dict of urls)

  $ hg log -rnull -T '{peerurls|json}\n' | sed 's|\\\\|/|g'
  {"dupe": "$TESTTMP/b#tip", "expand": "$TESTTMP/a/$SOMETHING/bar"}

password should be masked in plain output, but not in machine-readable/template
output:

  $ echo 'insecure = http://foo:insecure@example.com/' >> .hg/hgrc
  $ hg paths insecure
  http://foo:***@example.com/
  $ hg paths -Tjson insecure
  [
   {
    "name": "insecure",
    "url": "http://foo:insecure@example.com/"
   }
  ]
  $ hg log -rnull -T '{get(peerurls, "insecure")}\n'
  http://foo:insecure@example.com/

  $ cd ..

sub-options for an undeclared path are ignored

  $ hg init suboptions
  $ cd suboptions

  $ cat > .hg/hgrc << EOF
  > [paths]
  > path0 = https://example.com/path0
  > path1:pushurl = https://example.com/path1
  > EOF
  $ hg paths
  path0 = https://example.com/path0

unknown sub-options aren't displayed

  $ cat > .hg/hgrc << EOF
  > [paths]
  > path0 = https://example.com/path0
  > path0:foo = https://example.com/path1
  > EOF

  $ hg paths
  path0 = https://example.com/path0

:pushurl must be a URL

  $ cat > .hg/hgrc << EOF
  > [paths]
  > default = /path/to/nothing
  > default:pushurl = /not/a/url
  > EOF

  $ hg paths
  (paths.default:pushurl not a URL; ignoring)
  default = /path/to/nothing

