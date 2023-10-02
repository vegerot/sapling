/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use mononoke_types::path::MPath;
use mononoke_types::NonRootMPath;

use crate::PathTree;

#[derive(Debug, Clone)]
pub enum PathOrPrefix {
    Path(MPath),
    Prefix(MPath),
}

impl From<MPath> for PathOrPrefix {
    fn from(path: MPath) -> Self {
        PathOrPrefix::Path(path)
    }
}

impl From<NonRootMPath> for PathOrPrefix {
    fn from(path: NonRootMPath) -> Self {
        PathOrPrefix::Prefix(path.into())
    }
}

#[derive(Default)]
pub(crate) enum Select {
    /// Single entry selected
    Single,

    /// Whole substree selected
    Recursive,

    /// Not selected
    #[default]
    Skip,
}

impl Select {
    pub(crate) fn is_selected(&self) -> bool {
        match self {
            Select::Single | Select::Recursive => true,
            Select::Skip => false,
        }
    }

    pub(crate) fn is_recursive(&self) -> bool {
        match self {
            Select::Recursive => true,
            _ => false,
        }
    }
}

pub(crate) fn select_path_tree<I, P>(paths_or_prefixes: I) -> PathTree<Select>
where
    I: IntoIterator<Item = P>,
    PathOrPrefix: From<P>,
{
    paths_or_prefixes
        .into_iter()
        .map(|path_or_prefix| match PathOrPrefix::from(path_or_prefix) {
            PathOrPrefix::Path(path) => (path, Select::Single),
            PathOrPrefix::Prefix(path) => (path, Select::Recursive),
        })
        .collect()
}
