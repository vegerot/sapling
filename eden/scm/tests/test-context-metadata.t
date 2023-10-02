#debugruntest-compatible

  $ eagerepo
  $ setconfig devel.segmented-changelog-rev-compat=true
Tests about metadataonlyctx

  $ hg init
  $ echo A > A
  $ hg commit -A A -m 'Add A'
  $ echo B > B
  $ hg commit -A B -m 'Add B'
  $ hg rm A
  $ echo C > C
  $ echo B2 > B
  $ hg add C -q
  $ hg commit -m 'Remove A'

  $ cat > metaedit.py <<EOF
  > from __future__ import absolute_import
  > from sapling import context, registrar
  > cmdtable = {}
  > command = registrar.command(cmdtable)
  > @command('metaedit')
  > def metaedit(ui, repo, arg):
  >     # Modify commit message to "FOO"
  >     with repo.wlock(), repo.lock(), repo.transaction('metaedit'):
  >         old = repo['.']
  >         kwargs = dict(s.split('=', 1) for s in arg.split(';'))
  >         if 'parents' in kwargs:
  >             kwargs['parents'] = kwargs['parents'].split(',')
  >         new = context.metadataonlyctx(repo, old, **kwargs)
  >         new.commit()
  > EOF
  $ hg --config extensions.metaedit=$TESTTMP/metaedit.py metaedit 'text=Changed'
  $ hg log -r tip
  commit:      ad83e9e00ec9
  user:        test
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     Changed
  
  $ hg --config extensions.metaedit=$TESTTMP/metaedit.py metaedit 'parents=0' 2>&1 | egrep '^RuntimeError'
  RuntimeError: new p1 manifest (007d8c9d88841325f5c6b06371b35b4e8a2b1a83) is not the old p1 manifest (cb5cbbc1bfbf24cc34b9e8c16914e9caa2d2a7fd)

  $ hg --config extensions.metaedit=$TESTTMP/metaedit.py metaedit 'user=foo <foo@example.com>'
  $ hg log -r tip
  commit:      1f86eaeca92b
  user:        foo <foo@example.com>
  date:        Thu Jan 01 00:00:00 1970 +0000
  summary:     Remove A
  
