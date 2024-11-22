/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use staticconfig::static_config;
use staticconfig::StaticConfig;

/// Default config. Partially migrated from configitems.py.
///
/// Lowest priority. Should always be loaded.
pub static CONFIG: StaticConfig = static_config!("builtin:core" => r#"
[treestate]
mingcage=900
minrepackthreshold=10M
repackfactor=3

[ui]
timeout=600
color=auto
paginate=true
ignorerevnum=True

[checkout]
resumable=true

[tracing]
stderr=false
threshold=10

[format]
generaldelta=false
usegeneraldelta=true

[color]
status.added=green bold
status.clean=none
status.copied=none
status.deleted=cyan bold underline
status.ignored=black bold
status.modified=blue bold
status.removed=red bold
status.unknown=magenta bold underline

[commands]
naked-default.in-repo=sl
naked-default.no-repo=help

[git]
filter=blob:none

[unsafe]
filtersuspectsymlink=true

[experimental]
exportstack-max-bytes=1M

log-implicit-follow-threshold=10000

titles-namespace=true
local-committemplate=true

evalframe-passthrough=true

[zsh]
completion-age=7
completion-description=false

[merge]
enable-merge-tool-script=true

[remotenames]
autocleanupthreshold=50
selectivepulldefault=master
selectivepulldiscovery=true
autopullhoistpattern=
autopullpattern=re:^(?:default|remote)/[A-Za-z0-9._/-]+$
hoist=default

[scmstore]
handle-tree-parents=true

[filetype-patterns]
**/BUCK=buck
**.bzl=buck
**.php=hack
**.cpp=cpp
**.c=c
**.m=object-c
**.h=dot-h
**.py=python
**.js=javascript
**.ts=typescript
**.java=java
**.kt=kotlin
**.rs=rust
**.cs=csharp

[automerge]
merge-algos=adjacent-changes,subset-changes
mode=accept
import-pattern:buck=re:^\s*(".*//.*",|load\(.*)$
import-pattern:hack=re:^\s*use .*$
import-pattern:cpp=re:^\s*#include .*$
import-pattern:c=re:^\s*#include .*$
import-pattern:object-c=re:^\s*(#include|#import) .*$
import-pattern:dot-h=re:^\s*(#include|#import) .*$
import-pattern:python=re:^\s*import .*$
import-pattern:javascript=re:^\s*import .*$
import-pattern:typescript=re:^\s*import .*$
import-pattern:java=re:^\s*import .*$
import-pattern:kotlin=re:^\s*import .*$
import-pattern:rust=re:^\s*use .*$
import-pattern:csharp=re:^\s*using .*$
import-pattern:go=re:^\s*using .*$

[clone]
use-commit-graph=true
"#);
