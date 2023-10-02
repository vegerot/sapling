#debugruntest-compatible
#inprocess-hg-incompatible

  $ eagerepo
ensure that failing ui.atexit handlers report sensibly

  $ cat > $TESTTMP/bailatexit.py <<EOF
  > from sapling import util
  > def bail():
  >     raise RuntimeError('ui.atexit handler exception')
  > 
  > def extsetup(ui):
  >     ui.atexit(bail)
  > EOF
  $ hg -q --config extensions.bailatexit=$TESTTMP/bailatexit.py \
  >  help help
  hg help [-ecks] [TOPIC]
  
  show help for a given topic or a help overview
  error in exit handlers:
  Traceback (most recent call last):
    # collapsed by devel.collapse-traceback
  RuntimeError: ui.atexit handler exception
  [255]

  $ rm $TESTTMP/bailatexit.py

another bad extension

  $ echo 'raise Exception("bit bucket overflow")' > badext.py
  $ abspathexc=`pwd`/badext.py

  $ cat >baddocext.py <<EOF
  > """
  > baddocext is bad
  > """
  > EOF
  $ abspathdoc=`pwd`/baddocext.py

  $ cat <<EOF >> $HGRCPATH
  > [extensions]
  > badext = $abspathexc
  > baddocext = $abspathdoc
  > badext2 =
  > EOF

  $ hg -q help help 2>&1 |grep extension
  warning: extension badext is disabled because it cannot be imported from $TESTTMP/badext.py: bit bucket overflow
  warning: extension badext2 is disabled because it cannot be imported: No module named 'sapling.ext.badext2'

show traceback

  $ hg -q help help --traceback 2>&1 | egrep ' extension|^Exception|Traceback|ImportError'
  warning: extension badext is disabled because it cannot be imported from $TESTTMP/badext.py: bit bucket overflow
  Traceback (most recent call last):
  Exception: bit bucket overflow
  warning: extension badext2 is disabled because it cannot be imported: No module named 'sapling.ext.badext2'
  Traceback (most recent call last):

names of extensions failed to load can be accessed via extensions.notloaded()

  $ cat <<EOF > showbadexts.py
  > from sapling import commands, extensions, registrar
  > cmdtable = {}
  > command = registrar.command(cmdtable)
  > @command('showbadexts', norepo=True)
  > def showbadexts(ui, *pats, **opts):
  >     ui.write('BADEXTS: %s\n' % ' '.join(sorted(extensions.notloaded())))
  > EOF
  $ hg --config extensions.badexts=showbadexts.py showbadexts 2>&1 | grep '^BADEXTS'
  BADEXTS: badext badext2

show traceback for ImportError of ext.name if debug is set
(note that --debug option isn't applied yet when loading extensions)

  $ (hg -q help help --traceback --config ui.debug=True 2>&1) \
  > | grep -v '^ ' \
  > | egrep 'extension..[^p]|^Exception|Traceback|ImportError|not import'
  warning: extension badext is disabled because it cannot be imported from $TESTTMP/badext.py: bit bucket overflow
  Traceback (most recent call last):
  Exception: bit bucket overflow
  warning: extension badext2 is disabled because it cannot be imported: No module named 'sapling.ext.badext2'
  Traceback (most recent call last):

confirm that there's no crash when an extension's documentation is bad

  $ hg help --keyword baddocext
  warning: extension badext is disabled because it cannot be imported from $TESTTMP/badext.py: bit bucket overflow
  warning: extension badext2 is disabled because it cannot be imported: No module named 'sapling.ext.badext2'
  Topics:
  
   extensions Using Additional Features
