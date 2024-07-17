
#require no-eden


  $ eagerepo
Tests rebasing with part of the rebase set already in the
destination (issue5422)

  $ configure mutation-norecord
  $ enable rebase

  $ rebasewithdag() {
  >   N=$((N + 1))
  >   hg init repo$N && cd repo$N
  >   hg debugdrawdag
  >   hg rebase "$@" && tglog
  >   cd ..
  >   return $r
  > }

Rebase two commits, of which one is already in the right place

  $ rebasewithdag -r C+D -d B <<EOF
  > C
  > |
  > B D
  > |/
  > A
  > EOF
  rebasing b18e25de2cf5 "D" (D)
  already rebased 26805aba1e60 "C" (C)
  o  fe3b4c6498fa 'D' D
  │
  │ o  26805aba1e60 'C' C
  ├─╯
  o  112478962961 'B' B
  │
  o  426bada5c675 'A' A
  
Can collapse commits even if one is already in the right place

  $ rebasewithdag --collapse -r C+D -d B <<EOF
  > C
  > |
  > B D
  > |/
  > A
  > EOF
  rebasing b18e25de2cf5 "D" (D)
  rebasing 26805aba1e60 "C" (C)
  o  a2493f4ace65 'Collapsed revision
  │  * D
  │  * C' C D
  o  112478962961 'B' B
  │
  o  426bada5c675 'A' A
  
Rebase with "holes". The commits after the hole should end up on the parent of
the hole (B below), not on top of the destination (A).

  $ rebasewithdag -r B+D -d A <<EOF
  > D
  > |
  > C
  > |
  > B
  > |
  > A
  > EOF
  already rebased 112478962961 "B" (B)
  rebasing f585351a92f8 "D" (D)
  o  1e6da8103bc7 'D' D
  │
  │ o  26805aba1e60 'C' C
  ├─╯
  o  112478962961 'B' B
  │
  o  426bada5c675 'A' A
  
Abort doesn't lose the commits that were already in the right place

  $ newrepo abort
  $ hg debugdrawdag <<EOF
  > C
  > |
  > B D  # B/file = B
  > |/   # D/file = D
  > A
  > EOF
  $ hg rebase -r C+D -d B
  rebasing ef8c0fe0897b "D" (D)
  merging file
  warning: 1 conflicts while merging file! (edit, then use 'hg resolve --mark')
  unresolved conflicts (see hg resolve, then hg rebase --continue)
  [1]
  $ hg rebase --abort
  rebase aborted
  $ tglog
  o  79f6d6ab7b14 'C' C
  │
  │ o  ef8c0fe0897b 'D' D
  │ │
  o │  594087dbaf71 'B' B
  ├─╯
  o  426bada5c675 'A' A
  
