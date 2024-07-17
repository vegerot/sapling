/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {Disposable, Hash} from './types';
import type {ViteHotContext} from 'vite/types/hot';

export function firstOfIterable<T>(it: IterableIterator<T>): T | undefined {
  return it.next().value;
}

/** Get the short 12-character hash from a full hash. */
export function short(hash: Hash): string {
  return hash.slice(0, 12);
}

export function assert(shouldBeTrue: boolean, error: string): asserts shouldBeTrue {
  if (!shouldBeTrue) {
    throw new Error(error);
  }
}

export function arraysEqual<T>(a: Array<T>, b: Array<T>): boolean {
  if (a.length !== b.length) {
    return false;
  }
  return a.every((val, i) => b[i] === val);
}

export type NonNullReactElement = React.ReactElement | React.ReactFragment;

/**
 * name of the isl platform being used,
 * for example 'browser' or 'vscode'.
 * Note: This is exposed outisde of isl/platform.ts to prevent import cycles.
 */
export function islPlatformName(): string {
  return window.islPlatform?.platformName ?? 'browser';
}

export function getWindowWidthInPixels(): number {
  if (isTest) {
    return 1000;
  }
  // Use client width and not screen width to handle embedding as an iframe.
  return document.body.clientWidth;
}

export function leftPad(val: string | number, len: number, char: string) {
  const str = val.toString();
  return `${Array(len - str.length + 1).join(char)}${str}`;
}

/** Whether running in a test environment. */
export const isTest = typeof process !== 'undefined' && process.env.NODE_ENV === 'test';

export const isDev = process.env.NODE_ENV === 'development';

const cleanUpRegister = new FinalizationRegistry<() => void>((cleanUp: () => void) => {
  cleanUp();
});

/**
 * Register a clean up callback or a disposable when `obj` is GC-ed.
 *
 * If `hot` is set (`import.meta.hot`), the `cleanUp` is registered with the
 * hot reload API instead. Note the `import.meta` depends on where it lives.
 * So we cannot use `import.meta` here (which will affect this `utils.ts` hot
 * reloading behavior, not the callsite module).
 */
export function registerCleanup(obj: object, cleanUp: () => void, hot?: ViteHotContext): void {
  if (hot != null) {
    hot.dispose(() => {
      cleanUp();
    });
  } else {
    cleanUpRegister.register(obj, cleanUp);
  }
}

/** Similar to `registerCleanup`, but takes a `Disposable` */
export function registerDisposable(
  obj: object,
  disposable: Disposable,
  hot?: ViteHotContext,
): void {
  registerCleanup(obj, () => disposable.dispose(), hot);
}
