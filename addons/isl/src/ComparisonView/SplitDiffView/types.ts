/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {ThemeColor} from '../../theme';
import type {Result} from '../../types';
import type {Comparison} from 'shared/Comparison';

type ContextId = {path: string; comparison: Comparison};

/**
 * Context used to render SplitDiffView
 */
export type Context = {
  id: ContextId;
  copy?: (s: string) => void;
  openFile?: () => unknown;
  openFileToLine?: (line: OneIndexedLineNumber) => unknown;
  collapsed: boolean;
  setCollapsed: (collapsed: boolean) => void;
  fetchAdditionalLines?(
    id: ContextId,
    start: OneIndexedLineNumber,
    numLines: number,
  ): Promise<Result<Array<string>>>;
  /** A React hook that gives a string value used as an effect dependency. If this value changes, the comparison will be considered invalidated and must be refreshed.
   * This is a hook so it can trigger rerenders. */
  useComparisonInvalidationKeyHook?: () => string;
  /** A React hook that returns the current theme color. This is a hook so it can trigger rerenders, but can use atom values. */
  useThemeHook: () => ThemeColor;
  /** Translation function for the current language. */
  t?: (s: string) => string;
  /**
   * Whether to render as a side-by-side diff view, or a unified view where deleted and added lines are interleaved.
   * TODO: make this controllable / configurable / responsive based on screen width
   */
  display: 'split' | 'unified';
};

export type OneIndexedLineNumber = Exclude<number, 0>;
