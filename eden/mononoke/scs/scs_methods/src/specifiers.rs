/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use faster_hex::hex_string;
use source_control as thrift;

pub trait SpecifierExt: Send + Sync {
    fn description(&self) -> String;

    fn scuba_reponame(&self) -> Option<String> {
        None
    }

    fn scuba_commit(&self) -> Option<String> {
        None
    }

    fn scuba_path(&self) -> Option<String> {
        None
    }
}

impl SpecifierExt for thrift::RepoSpecifier {
    fn description(&self) -> String {
        format!("repo={}", self.name)
    }

    fn scuba_reponame(&self) -> Option<String> {
        Some(self.name.clone())
    }
}

impl SpecifierExt for thrift::CommitSpecifier {
    fn description(&self) -> String {
        format!("repo={} commit={}", self.repo.name, self.id)
    }

    fn scuba_reponame(&self) -> Option<String> {
        self.repo.scuba_reponame()
    }

    fn scuba_commit(&self) -> Option<String> {
        Some(self.id.to_string())
    }
}

impl SpecifierExt for thrift::CommitPathSpecifier {
    fn description(&self) -> String {
        format!(
            "repo={} commit={} path={}",
            self.commit.repo.name, self.commit.id, self.path
        )
    }

    fn scuba_reponame(&self) -> Option<String> {
        self.commit.scuba_reponame()
    }
    fn scuba_commit(&self) -> Option<String> {
        self.commit.scuba_commit()
    }
    fn scuba_path(&self) -> Option<String> {
        Some(self.path.clone())
    }
}

impl SpecifierExt for thrift::TreeSpecifier {
    fn description(&self) -> String {
        match self {
            thrift::TreeSpecifier::by_commit_path(commit_path) => commit_path.description(),
            thrift::TreeSpecifier::by_id(tree_id) => format!(
                "repo={} tree={}",
                tree_id.repo.name,
                hex_string(&tree_id.id)
            ),
            thrift::TreeSpecifier::UnknownField(n) => format!("unknown tree specifier type {}", n),
        }
    }

    fn scuba_reponame(&self) -> Option<String> {
        match self {
            thrift::TreeSpecifier::by_commit_path(commit_path) => commit_path.scuba_reponame(),
            thrift::TreeSpecifier::by_id(tree_id) => tree_id.repo.scuba_reponame(),
            thrift::TreeSpecifier::UnknownField(_) => None,
        }
    }

    fn scuba_commit(&self) -> Option<String> {
        match self {
            thrift::TreeSpecifier::by_commit_path(commit_path) => commit_path.scuba_commit(),
            thrift::TreeSpecifier::by_id(_tree_id) => None,
            thrift::TreeSpecifier::UnknownField(_) => None,
        }
    }

    fn scuba_path(&self) -> Option<String> {
        match self {
            thrift::TreeSpecifier::by_commit_path(commit_path) => commit_path.scuba_path(),
            thrift::TreeSpecifier::by_id(_tree_id) => None,
            thrift::TreeSpecifier::UnknownField(_) => None,
        }
    }
}

impl SpecifierExt for thrift::FileSpecifier {
    fn description(&self) -> String {
        match self {
            thrift::FileSpecifier::by_commit_path(commit_path) => commit_path.description(),
            thrift::FileSpecifier::by_id(file_id) => format!(
                "repo={} file={}",
                file_id.repo.name,
                hex_string(&file_id.id),
            ),
            thrift::FileSpecifier::by_sha1_content_hash(hash) => format!(
                "repo={} file_sha1={}",
                hash.repo.name,
                hex_string(&hash.content_hash),
            ),
            thrift::FileSpecifier::by_sha256_content_hash(hash) => format!(
                "repo={} file_sha256={}",
                hash.repo.name,
                hex_string(&hash.content_hash),
            ),
            thrift::FileSpecifier::UnknownField(n) => format!("unknown file specifier type {}", n),
        }
    }

    fn scuba_reponame(&self) -> Option<String> {
        match self {
            thrift::FileSpecifier::by_commit_path(commit_path) => commit_path.scuba_reponame(),
            thrift::FileSpecifier::by_id(file_id) => file_id.repo.scuba_reponame(),
            thrift::FileSpecifier::by_sha1_content_hash(hash) => hash.repo.scuba_reponame(),
            thrift::FileSpecifier::by_sha256_content_hash(hash) => hash.repo.scuba_reponame(),
            thrift::FileSpecifier::UnknownField(_) => None,
        }
    }
    fn scuba_commit(&self) -> Option<String> {
        match self {
            thrift::FileSpecifier::by_commit_path(commit_path) => commit_path.scuba_commit(),
            thrift::FileSpecifier::by_id(_file_id) => None,
            thrift::FileSpecifier::by_sha1_content_hash(_hash) => None,
            thrift::FileSpecifier::by_sha256_content_hash(_hash) => None,
            thrift::FileSpecifier::UnknownField(_) => None,
        }
    }
    fn scuba_path(&self) -> Option<String> {
        match self {
            thrift::FileSpecifier::by_commit_path(commit_path) => commit_path.scuba_path(),
            thrift::FileSpecifier::by_id(file_id) => Some(hex_string(&file_id.id)),
            thrift::FileSpecifier::by_sha1_content_hash(hash) => {
                Some(hex_string(&hash.content_hash))
            }
            thrift::FileSpecifier::by_sha256_content_hash(hash) => {
                Some(hex_string(&hash.content_hash))
            }
            thrift::FileSpecifier::UnknownField(_) => None,
        }
    }
}
