
#require no-eden

#inprocess-hg-incompatible

  $ newclientrepo repo
  $ export EDENSCM_LOG=auth=debug

No credentials

  $ hg debugreadauthforuri https://example.com
  no match found

Single credential

  $ cp $HGRCPATH $TESTTMP/orig.hrc
  $ cat >> "$HGRCPATH" << EOF
  > [auth]
  > first.prefix=example.com
  > first.schemes=https
  > EOF

  $ hg debugreadauthforuri https://example.com
  DEBUG auth: best_auth_group url=https://example.com/ best=AuthGroup { name: "first", prefix: "example.com", cert: None, key: None, cacerts: None, username: None, schemes: ["https"], priority: 0, extras: {} }
  auth.first.prefix=example.com
  auth.first.schemes=https

Non-existent credentials are ignored

  $ cp $TESTTMP/orig.hrc $HGRCPATH
  $ cat >> "$HGRCPATH" << EOF
  > [auth]
  > first.cert=foocert
  > first.prefix=example.com
  > first.schemes=https
  > second.key=fookey
  > second.prefix=example.com
  > second.schemes=https
  > EOF

  $ hg debugreadauthforuri https://example.com
  DEBUG auth: Ignoring [auth] group "first" because of missing client certificate: "foocert"
  DEBUG auth: Ignoring [auth] group "second" because of missing private key: "fookey"
  no match found

Valid credentials are used

  $ touch foocert
  $ cp $TESTTMP/orig.hrc $HGRCPATH
  $ cat >> "$HGRCPATH" << EOF
  > [auth]
  > first.cert=foocert
  > first.prefix=example.com
  > first.schemes=https
  > EOF

  $ hg debugreadauthforuri https://example.com
  DEBUG auth: best_auth_group url=https://example.com/ best=AuthGroup { name: "first", prefix: "example.com", cert: Some("foocert"), key: None, cacerts: None, username: None, schemes: ["https"], priority: 0, extras: {} }
  auth.first.cert=foocert
  auth.first.prefix=example.com
  auth.first.schemes=https

Valid credentials are preferred

  $ cp $TESTTMP/orig.hrc $HGRCPATH
  $ cat >> "$HGRCPATH" << EOF
  > [auth]
  > first.cert=foocert
  > first.prefix=example.com
  > first.schemes=https
  > second.key=fookey
  > second.prefix=example.com
  > second.schemes=https
  > second.priority=1
  > EOF

  $ hg debugreadauthforuri https://example.com
  DEBUG auth: Ignoring [auth] group "second" because of missing private key: "fookey"
  DEBUG auth: best_auth_group url=https://example.com/ best=AuthGroup { name: "first", prefix: "example.com", cert: Some("foocert"), key: None, cacerts: None, username: None, schemes: ["https"], priority: 0, extras: {} }
  auth.first.cert=foocert
  auth.first.prefix=example.com
  auth.first.schemes=https

Longest prefixes are used

  $ cp $TESTTMP/orig.hrc $HGRCPATH
  $ cat >> "$HGRCPATH" << EOF
  > [auth]
  > first.prefix=example.com/foo
  > first.schemes=https
  > second.prefix=example.com
  > second.schemes=https
  > EOF

  $ hg debugreadauthforuri https://example.com/foo
  DEBUG auth: best_auth_group url=https://example.com/foo best=AuthGroup { name: "first", prefix: "example.com/foo", cert: None, key: None, cacerts: None, username: None, schemes: ["https"], priority: 0, extras: {} }
  auth.first.prefix=example.com/foo
  auth.first.schemes=https

Prefixes take precedence over priorities

  $ cp $TESTTMP/orig.hrc $HGRCPATH
  $ cat >> "$HGRCPATH" << EOF
  > [auth]
  > first.prefix=example.com/foo
  > first.schemes=https
  > second.prefix=example.com
  > second.schemes=https
  > second.priority=1
  > EOF

  $ hg debugreadauthforuri https://example.com/foo
  DEBUG auth: best_auth_group url=https://example.com/foo best=AuthGroup { name: "first", prefix: "example.com/foo", cert: None, key: None, cacerts: None, username: None, schemes: ["https"], priority: 0, extras: {} }
  auth.first.prefix=example.com/foo
  auth.first.schemes=https

Priorities take precedence over user names

  $ cp $TESTTMP/orig.hrc $HGRCPATH
  $ cat >> "$HGRCPATH" << EOF
  > [auth]
  > first.prefix=example.com
  > first.schemes=https
  > first.username=user
  > second.prefix=example.com
  > second.schemes=https
  > second.priority=1
  > EOF

  $ hg debugreadauthforuri https://example.com
  DEBUG auth: best_auth_group url=https://example.com/ best=AuthGroup { name: "second", prefix: "example.com", cert: None, key: None, cacerts: None, username: None, schemes: ["https"], priority: 1, extras: {} }
  auth.second.prefix=example.com
  auth.second.priority=1
  auth.second.schemes=https

User names are used if everything else matches

  $ cp $TESTTMP/orig.hrc $HGRCPATH
  $ cat >> "$HGRCPATH" << EOF
  > [auth]
  > first.prefix=example.com
  > first.schemes=https
  > first.username=user
  > first.priority=1
  > second.prefix=example.com
  > second.schemes=https
  > second.priority=1
  > EOF

  $ hg debugreadauthforuri https://example.com
  DEBUG auth: best_auth_group url=https://example.com/ best=AuthGroup { name: "first", prefix: "example.com", cert: None, key: None, cacerts: None, username: Some("user"), schemes: ["https"], priority: 1, extras: {} }
  auth.first.prefix=example.com
  auth.first.priority=1
  auth.first.schemes=https
  auth.first.username=user
