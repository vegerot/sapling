/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use async_runtime::block_on;
use async_runtime::block_unless_interrupted;
use clidispatch::errors;
use clidispatch::ReqCtx;
use cliparser::define_flags;
use cmdutil::ConfigSet;
use cmdutil::Result;
use dag::namedag::IndexedLogNameDagPath;
use dag::ops::DagImportCloneData;
use dag::ops::DagPersistent;
use dag::ops::Open;
use dag::CloneData;
use dag::Group;
use dag::VertexListWithOptions;
use dag::VertexName;
use progress_model::ProgressBar;

define_flags! {
    pub struct StatusOpts {
        #[arg]
        reponame: String,

        #[arg]
        dest: String,
    }
}
pub fn run(ctx: ReqCtx<StatusOpts>) -> Result<u8> {
    let mut config = ConfigSet::wrap(ctx.config().clone());

    let reponame = ctx.opts.reponame;
    let destination = PathBuf::from(&ctx.opts.dest);

    if destination.exists() {
        return Err(
            errors::Abort(format!("destination {} exists", destination.display()).into()).into(),
        );
    }

    config.set(
        "remotefilelog",
        "reponame",
        Some(reponame.clone()),
        &"arg".into(),
    );
    let edenapi_client = edenapi::Builder::from_config(&config)?.build()?;

    let clone_data = match block_unless_interrupted(edenapi_client.clone_data()) {
        Err(e) => Err(anyhow::Error::from(e)),
        Ok(Err(e)) => Err(anyhow::Error::from(e)),
        Ok(Ok(v)) => Ok(v),
    }
    .context("error cloning segmented changelog")?;

    let ident = identity::default();
    let dot_path = destination.join(ident.dot_dir());

    let mut segments_path = dot_path.clone();
    segments_path.push("store");
    segments_path.push("segments");
    segments_path.push("v1");
    let namedag_path = IndexedLogNameDagPath(segments_path);

    let mut namedag = namedag_path
        .open()
        .context("error opening segmented changelog")?;

    let len = clone_data.idmap.len();
    let bar = ProgressBar::new_adhoc("Building", len as _, "commits");
    let idmap: BTreeMap<_, _> = clone_data
        .idmap
        .into_iter()
        .map(|(k, v)| {
            bar.increase_position(1);
            (k, VertexName::copy_from(&v.into_byte_array()))
        })
        .collect();

    let master = idmap.iter().max_by_key(|i| i.0).map(|i| i.1.clone());
    if let Some(master) = master {
        let vertex_clone_data = CloneData {
            flat_segments: clone_data.flat_segments,
            idmap,
        };
        block_on(namedag.import_clone_data(vertex_clone_data))
            .context("error importing segmented changelog")?;

        let heads =
            VertexListWithOptions::from(vec![master.clone()]).with_desired_group(Group::MASTER);
        block_on(namedag.flush(&heads)).context("error writing segmented changelog to disk")?;

        let mut remotenames_path = dot_path.clone();
        remotenames_path.push("store");
        remotenames_path.push("remotenames");

        fs::write(
            remotenames_path,
            format!("{} bookmarks remote/master\n", master.to_hex()).as_bytes(),
        )
        .context("error writing to remotenames")?;
    }

    fs::write(
        dot_path.join("requires"),
        b"dotencode\n\
          fncache\n\
          generaldelta\n\
          remotefilelog\n\
          store\n\
          treestate\n",
    )
    .context("error writing to requires")?;

    let mut store_requires_path = dot_path.clone();
    store_requires_path.push("store");
    store_requires_path.push("requires");

    fs::write(
        store_requires_path,
        b"lazychangelog\n\
          narrowheads\n\
          visibleheads\n",
    )
    .context("error writing to store requires")?;

    fs::write(
        dot_path.join(ident.config_repo_file()),
        format!(
            "[paths]\n\
             default = ssh://hg.vip.facebook.com//data/scm/{0}\n\
             %include /etc/mercurial/repo-specific/{0}.rc\n",
            reponame
        )
        .as_bytes(),
    )
    .context("error writing to store requires")?;

    Ok(0)
}

pub fn aliases() -> &'static str {
    "debugsegmentclone"
}

pub fn doc() -> &'static str {
    "clone a repository using segmented changelog"
}

pub fn synopsis() -> Option<&'static str> {
    None
}
