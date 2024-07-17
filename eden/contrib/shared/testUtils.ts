/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {Json} from './typeUtils';

export interface Logger {
  info(...args: Parameters<typeof console.info>): void;
  log(...args: Parameters<typeof console.log>): void;
  warn(...args: Parameters<typeof console.warn>): void;
  error(...args: Parameters<typeof console.error>): void;
}

export const mockLogger: Logger = {
  log: jest.fn(),
  info: jest.fn(),
  warn: jest.fn(),
  error: jest.fn(),
};

export function clone<T extends Json>(o: T): T {
  return JSON.parse(JSON.stringify(o));
}

/**
 * Returns a Promise which resolves after the current async tick is finished.
 * Useful for testing code which `await`s.
 */
export function nextTick(): Promise<void> {
  return new Promise(res => setTimeout(res, 0));
}
