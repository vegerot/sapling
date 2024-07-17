/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {ConfigName, LocalStorageName, SettableConfigName} from './types';
import type {WritableAtom, Atom, Getter} from 'jotai';
import type {Json} from 'shared/typeUtils';

import serverAPI from './ClientToServerAPI';
import platform from './platform';
import {assert} from './utils';
import {atom, getDefaultStore, useAtomValue} from 'jotai';
import {loadable} from 'jotai/utils';
import {useMemo} from 'react';
import {RateLimiter} from 'shared/RateLimiter';
import {isPromise} from 'shared/utils';

/** A mutable atom that stores type `T`. */
export type MutAtom<T> = WritableAtom<T, [T | ((prev: T) => T)], void>;

/**
 * The store being used. Do not use this directly. Alternatives are:
 * - use `readAtom` instead of `store.get`.
 * - use `writeAtom` instead of `store.set`.
 * - use `atomWithOnChange` instead of `store.sub`.
 */
let store = getDefaultStore();

/**
 * Replace the current Jotai store used by this module.
 * Practically, this is only useful for tests to reset states.
 */
export function setJotaiStore(newStore: typeof store) {
  store = newStore;
}

/** Define a read-write atom backed by a config. */
export function configBackedAtom<T extends Json>(
  name: SettableConfigName,
  defaultValue: T,
  readonly?: false,
): MutAtom<T>;

/**
 * Define a read-only atom backed by a config.
 *
 * This can be useful for staged rollout features
 * where the config is not supposed to be set by the user.
 * (user config will override the staged rollout config)
 */
export function configBackedAtom<T extends Json>(
  name: ConfigName,
  defaultValue: T,
  readonly: true,
  useRawValue?: boolean,
): Atom<T>;

export function configBackedAtom<T extends Json>(
  name: ConfigName | SettableConfigName,
  defaultValue: T,
  readonly = false,
  useRawValue?: boolean,
): MutAtom<T> | Atom<T> {
  // https://jotai.org/docs/guides/persistence
  const primitiveAtom = atom<T>(defaultValue);

  let lastStrValue: undefined | string = undefined;
  serverAPI.onMessageOfType('gotConfig', event => {
    if (event.name !== name) {
      return;
    }
    lastStrValue = event.value;
    writeAtom(
      primitiveAtom,
      event.value === undefined
        ? defaultValue
        : useRawValue === true
        ? event.value
        : JSON.parse(event.value),
    );
  });
  serverAPI.onConnectOrReconnect(() => {
    serverAPI.postMessage({
      type: 'getConfig',
      name: name as ConfigName,
    });
  });

  return readonly
    ? atom<T>(get => get(primitiveAtom))
    : atom<T, [T | ((prev: T) => T)], void>(
        get => get(primitiveAtom),
        (get, set, update) => {
          const newValue = typeof update === 'function' ? update(get(primitiveAtom)) : update;
          set(primitiveAtom, newValue);
          const strValue = useRawValue ? String(newValue) : JSON.stringify(newValue);
          if (strValue !== lastStrValue) {
            lastStrValue = strValue;
            serverAPI.postMessage({
              type: 'setConfig',
              name: name as SettableConfigName,
              value: strValue,
            });
          }
        },
      );
}

/**
 * Loads this atom from a local persistent cache (usually browser local storage),
 * and persists any changes back to it.
 * Useful for some customizations that don't warrant a user-visible sl config,
 * for example UI expansion state.
 */
export function localStorageBackedAtom<T extends Json>(
  name: LocalStorageName,
  defaultValue: T,
): MutAtom<T> {
  const primitiveAtom = atom<T>(platform.getPersistedState<T>(name) ?? defaultValue);

  return atom(
    get => get(primitiveAtom),
    (get, set, update) => {
      const newValue = typeof update === 'function' ? update(get(primitiveAtom)) : update;
      set(primitiveAtom, newValue);
      platform.setPersistedState(name, newValue);
    },
  );
}

/**
 * Wraps an atom with an "onChange" callback.
 * Changing the returned atom will trigger the callback.
 * Calling this function will trigger `onChange` with the current value except when `skipInitialCall` is `true`.
 */
export function atomWithOnChange<T>(
  originalAtom: MutAtom<T>,
  onChange: (value: T) => void,
  skipInitialCall?: boolean,
): MutAtom<T> {
  if (skipInitialCall !== true) {
    onChange(readAtom(originalAtom));
  }
  return atom(
    get => get(originalAtom),
    (get, set, args) => {
      const oldValue = get(originalAtom);
      set(originalAtom, args);
      const newValue = get(originalAtom);
      if (oldValue !== newValue) {
        onChange(newValue);
      }
    },
  );
}

/**
 * Creates a lazily initialized atom.
 * On first read, trigger `load` to get the actual value.
 * `fallback` provides the value when the async `load` is running.
 * `original` is an optioinal nullable atom to provide the value.
 */
export function lazyAtom<T>(
  load: (get: Getter) => Promise<T> | T,
  fallback: T,
  original?: MutAtom<T | undefined>,
): MutAtom<T> {
  const originalAtom = original ?? atom<T | undefined>(undefined);
  const limiter = new RateLimiter(1);
  return atom(
    get => {
      const value = get(originalAtom);
      if (value !== undefined) {
        return value;
      }
      const loaded = load(get);
      if (!isPromise(loaded)) {
        writeAtom(originalAtom, loaded);
        return loaded;
      }
      // Kick off the "load" but rate limit it.
      limiter.enqueueRun(async () => {
        if (get(originalAtom) !== undefined) {
          // A previous "load" was completed.
          return;
        }
        const newValue = await loaded;
        writeAtom(originalAtom, newValue);
      });
      // Use the fallback value while waiting for the promise.
      return fallback;
    },
    (get, set, args) => {
      const newValue =
        typeof args === 'function' ? (args as (prev: T) => T)(get(originalAtom) ?? fallback) : args;
      set(originalAtom, newValue);
    },
  );
}

export function readAtom<T>(atom: Atom<T>): T {
  return store.get(atom);
}

export function writeAtom<T>(atom: MutAtom<T>, value: T | ((prev: T) => T)) {
  store.set(atom, value);
}

export function refreshAtom<T>(atom: WritableAtom<T, [], void>) {
  store.set(atom);
}

/** Create an atom that is automatically reset when the depAtom is changed. */
export function atomResetOnDepChange<T>(defaultValue: T, depAtom: Atom<unknown>): MutAtom<T> {
  assert(
    typeof depAtom !== 'undefined',
    'depAtom should not be undefined (is there a circular dependency?)',
  );
  const primitiveAtom = atom<T>(defaultValue);
  let lastDep = readAtom(depAtom);
  return atom(
    get => {
      const dep = get(depAtom);
      if (dep !== lastDep) {
        lastDep = dep;
        writeAtom(primitiveAtom, defaultValue);
      }
      return get(primitiveAtom);
    },
    (_get, set, update) => {
      set(primitiveAtom, update);
    },
  );
}

/**
 * Creates a derived atom that can be force-refreshed, by using the update function.
 * Uses Suspense for async update functions.
 * ```
 * const postsAtom = atomWithRefresh(get => fetchPostsAsync());
 *   ...
 * const [posts, refreshPosts] = useAtom(postsAtom);
 * ```
 */
export function atomWithRefresh<T>(fn: (get: Getter) => T) {
  const refreshCounter = atom(0);

  return atom(
    get => {
      get(refreshCounter);
      return fn(get);
    },
    (_, set) => set(refreshCounter, i => i + 1),
  );
}

/**
 * Creates a derived atom that can be force-refreshed, by using the update function.
 * The underlying async state is given as a Loadable atom instead of one that suspends.
 * ```
 * const postsAtom = atomWithRefresh(get => fetchPostsAsync());
 *   ...
 * const [postsLoadable, refreshPosts] = useAtom(postsAtom);
 * if (postsLoadable.state === 'hasData') {
 *   const posts = postsLoadable.data;
 * }
 * ```
 */
export function atomLoadableWithRefresh<T>(fn: (get: Getter) => Promise<T>) {
  const refreshCounter = atom(0);
  const loadableAtom = loadable(
    atom(get => {
      get(refreshCounter);
      return fn(get);
    }),
  );

  return atom(
    get => get(loadableAtom),
    (_, set) => set(refreshCounter, i => i + 1),
  );
}

/**
 * Drop-in replacement of `atomFamily` that tries to book-keep internal cache
 * periodically to avoid memory leak.
 *
 * There are 2 caches:
 * - "strong" cache: keep atoms alive even if all references are gone.
 * - "weak" cache: keep atoms alive as long as there is a reference to it.
 *
 * Periodically, when the weak cache size reaches a threshold, a "cleanup"
 * process runs to:
 * - Clear the "strong" cache to mitigate memory leak.
 * - Drop dead entries in the "weak" cache.
 * - Update the threshold to 2x the "weak" cache size.
 *
 * Therefore the memory waste is hopefully within 2x of the needed memory.
 *
 * Setting `options.useStrongCache` to `false` disables the "strong" cache
 * to further limit memory usage.
 */
export function atomFamilyWeak<K, A extends Atom<unknown>>(
  fn: (key: K) => A,
  options?: AtomFamilyWeakOptions,
): AtomFamilyWeak<K, A> {
  const {useStrongCache = true, initialCleanupThreshold = 4} = options ?? {};

  // This cache persists through component unmount / remount, therefore
  // it can be memory leaky.
  const strongCache = new Map<K, A>();

  // This cache ensures atoms in use are returned as-is during re-render,
  // to avoid infinite re-render with React StrictMode.
  const weakCache = new Map<K, WeakRef<A>>();

  const cleanup = () => {
    // Clear the strong cache. This allows GC to drop weakRefs.
    strongCache.clear();
    // Clean up weak cache - remove dead entries.
    weakCache.forEach((weakRef, key) => {
      if (weakRef.deref() == null) {
        weakCache.delete(key);
      }
    });
    // Adjust threshold to trigger the next cleanup.
    resultFunc.threshold = weakCache.size * 2;
  };

  const resultFunc: AtomFamilyWeak<K, A> = (key: K) => {
    const cached = strongCache.get(key);
    if (cached != null) {
      // This state was accessed recently.
      return cached;
    }
    const weakCached = weakCache.get(key)?.deref();
    if (weakCached != null) {
      // State is not dead yet.
      return weakCached;
    }
    // Not in cache. Need re-calculate.
    const atom = fn(key);
    if (useStrongCache) {
      strongCache.set(key, atom);
    }
    weakCache.set(key, new WeakRef(atom));
    if (weakCache.size > resultFunc.threshold) {
      cleanup();
    }
    if (resultFunc.debugLabel != null && atom.debugLabel == null) {
      atom.debugLabel = `${resultFunc.debugLabel}:${key}`;
    }
    return atom;
  };

  resultFunc.cleanup = cleanup;
  resultFunc.threshold = initialCleanupThreshold;
  resultFunc.strongCache = strongCache;
  resultFunc.weakCache = weakCache;
  resultFunc.clear = () => {
    weakCache.clear();
    strongCache.clear();
    resultFunc.threshold = initialCleanupThreshold;
  };

  return resultFunc;
}

type AtomFamilyWeakOptions = {
  /**
   * Enable the "strong" cache so unmount / remount can try to reuse the cache.
   *
   * If this is disabled, then only the weakRef cache is used, states that
   * are no longer referred by components might be lost to GC.
   *
   * Default: true.
   */
  useStrongCache?: boolean;

  /**
   * Number of items before triggering an initial cleanup.
   * Default: 4.
   */
  initialCleanupThreshold?: number;
};

export interface AtomFamilyWeak<K, A extends Atom<unknown>> {
  (key: K): A;
  /** The "strong" cache (can be empty). */
  strongCache: Map<K, A>;
  /** The weakRef cache (must contain entries that are still referred elsewhere). */
  weakCache: Map<K, WeakRef<A>>;
  /** Trigger a cleanup ("GC"). */
  cleanup(): void;
  /** Clear the cache */
  clear(): void;
  /** Auto cleanup threshold on weakCache size. */
  threshold: number;
  /** Prefix of debugLabel. */
  debugLabel?: string;
}

function setDebugLabelForDerivedAtom<A extends Atom<unknown>>(
  original: Atom<unknown>,
  derived: A,
  key: unknown,
): A {
  derived.debugLabel = `${original.debugLabel ?? original.toString()}:${key}`;
  return derived;
}

/**
 * Similar to `useAtomValue(mapAtom).get(key)` but avoids re-render if the map
 * is changed but the `get(key)` does not change.
 *
 * This might be an appealing alternative to `atomFamilyWeak` in some cases.
 * The `atomFamilyWeak` keeps caching state within itself and it has
 * undesirable memory overhead regardless of settings. This function makes
 * the hook own the caching state so states can be released cleanly on unmount.
 */
export function useAtomGet<K, V>(
  mapAtom: Atom<{get(k: K): V | undefined}>,
  key: K,
): Awaited<V | undefined> {
  const derivedAtom = useMemo(() => {
    const derived = atom(get => get(mapAtom).get(key));
    return setDebugLabelForDerivedAtom(mapAtom, derived, key);
  }, [key, mapAtom]);
  return useAtomValue(derivedAtom);
}

/**
 * Similar to `useAtomValue(setAtom).has(key)` but avoids re-render if the set
 * is changed but the `has(key)` does not change.
 *
 * This might be an appealing alternative to `atomFamilyWeak`. See `useAtomGet`
 * for explanation.
 */
export function useAtomHas<K>(setAtom: Atom<{has(k: K): boolean}>, key: K): Awaited<boolean> {
  const derivedAtom = useMemo(() => atom(get => get(setAtom).has(key)), [key, setAtom]);
  return useAtomValue(derivedAtom);
}
