/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

CREATE TABLE IF NOT EXISTS `megarepo_changeset_mapping` (
  `mapping_id` INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
  `source_name` VARCHAR(255) NOT NULL,
  `target_repo_id` INTEGER NOT NULL,
  `target_bookmark` VARCHAR(512) NOT NULL,
  `source_bcs_id` BINARY(32) NOT NULL,
  `target_bcs_id` BINARY(32) NOT NULL,
  `sync_config_version` VARCHAR(255),
  UNIQUE (`target_repo_id`,`target_bookmark`,`target_bcs_id`)
);
