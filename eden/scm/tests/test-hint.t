#debugruntest-compatible
  $ eagerepo
`
  $ newext showhint << EOF
  > from sapling import (
  >     cmdutil,
  >     hintutil,
  >     registrar,
  > )
  > 
  > cmdtable = {}
  > command = registrar.command(cmdtable)
  > 
  > hint = registrar.hint()
  > 
  > @hint('next')
  > def hintnext(a, b):
  >     return "use 'hg next' to go from %s to %s" % (a, b)
  > 
  > @hint('export')
  > def hintexport(a):
  >     return "use 'hg export %s' to show commit content" % (a,)
  > 
  > @hint('slow')
  > def hintslow(a):
  >     return "%r is slow - be patient" % (a,)
  > 
  > @command('showhint', norepo=True)
  > def showhint(ui, *args):
  >     hintutil.trigger('export', 'P')
  >     hintutil.triggershow(ui, 'slow', 'date(x)')
  >     hintutil.trigger('next', 'X', 'Y')
  >     hintutil.trigger('export', 'Q')
  > EOF

  $ hg showhint
  hint[slow]: 'date(x)' is slow - be patient
  hint[export]: use 'hg export P' to show commit content
  hint[next]: use 'hg next' to go from X to Y
  hint[hint-ack]: use 'hg hint --ack export next' to silence these hints

Test HGPLAIN=1 or HGPLAIN=hint silences all hints

  $ HGPLAIN=1 hg showhint
  $ HGPLAIN=hint hg showhint

Test silence configs

  $ hg showhint --config hint.ack-export=True --config hint.ack-slow=True
  hint[next]: use 'hg next' to go from X to Y
  hint[hint-ack]: use 'hg hint --ack next' to silence these hints
  $ hg showhint --config hint.ack=next
  hint[slow]: 'date(x)' is slow - be patient
  hint[export]: use 'hg export P' to show commit content
  hint[hint-ack]: use 'hg hint --ack export' to silence these hints
  $ hg showhint --config hint.ack=*

Test hint --ack command

  $ HGRCPATH=$HGRCPATH:$HOME/.hgrc
  $ hg hint --ack next hint-ack
  hints about next, hint-ack are silenced
  $ cat .hgrc
  [hint]
  ack = next hint-ack

  $ hg showhint
  hint[slow]: 'date(x)' is slow - be patient
  hint[export]: use 'hg export P' to show commit content

  $ hg hint --ack export slow -q
  $ cat .hgrc
  [hint]
  ack = next hint-ack export slow

  $ hg showhint
