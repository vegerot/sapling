/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {ServerSideTracker} from './analytics/serverSideTracker';
import type {Logger} from './logger';
import type {ConfigName} from 'isl/src/types';

/**
 * Per-connection context with which to access a repository.
 * Repositories instances are shared and reused, but
 * this context is not. It's used for any state that cannot be shared.
 */
export type RepositoryContext = {
  cmd: string;
  cwd: string;
  logger: Logger;
  tracker: ServerSideTracker;

  knownConfigs?: ReadonlyMap<ConfigName, string> | undefined;
  /**
   * Configured merge tool, from `ui.merge`, as long as merge-tools.$tool.gui is also True.
   * null -> no merge tool configured or not valid
   * undefined -> not cached yet
   */
  cachedMergeTool?: string | null;
  // TODO: visible commit age range
};
