/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use tempfile::tempdir;

use crate::util::read_or_generate_access_token;
use crate::util::TOKEN_FILENAME;

#[test]
fn test_read_access_token_from_file_should_return_token() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(TOKEN_FILENAME);
    let mut tmp = File::create(path).unwrap();
    writeln!(tmp, "[commitcloud]").unwrap();
    writeln!(tmp, "user_token=token").unwrap();
    let result = read_or_generate_access_token(&Some(PathBuf::from(dir.path()))).unwrap();
    drop(tmp);
    dir.close().unwrap();
    assert_eq!(result.token, "token");
}
