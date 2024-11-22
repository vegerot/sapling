/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::RwLock;

use futures::Future;
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;

pub struct Counter {
    name: &'static str,
    inner: AtomicUsize,
    registered: OnceCell<()>,
    gauge: bool,
}

impl Counter {
    /// By convention metric name should be crate.metric_name
    /// Metrics without '.' in name are not allowed (cause compilation error)
    pub const fn new_counter(name: &'static str) -> Self {
        // Unfortunately we can't check name this here because of const restriction
        Self {
            name,
            inner: AtomicUsize::new(0),
            registered: OnceCell::new(),
            gauge: false,
        }
    }

    pub const fn new_gauge(name: &'static str) -> Self {
        let mut counter = Self::new_counter(name);
        counter.gauge = true;
        counter
    }

    pub fn increment(&'static self) {
        self.add(1);
    }

    pub fn add(&'static self, val: usize) {
        self.inner().fetch_add(val, Ordering::Relaxed);
    }

    pub fn sub(&'static self, val: usize) {
        self.inner().fetch_sub(val, Ordering::Relaxed);
    }

    pub fn value(&'static self) -> usize {
        self.inner().load(Ordering::Relaxed)
    }

    /// Increment counter by v and decrement it back by v when returned guard is dropped
    pub fn entrance_guard(&'static self, v: usize) -> EntranceGuard {
        self.add(v);
        EntranceGuard(self, v)
    }

    pub fn is_gauge(&'static self) -> bool {
        self.gauge
    }

    fn inner(&'static self) -> &AtomicUsize {
        self.registered
            .get_or_init(|| Registry::global().register_counter(self));
        &self.inner
    }
}

pub struct EntranceGuard(&'static Counter, usize);

impl Drop for EntranceGuard {
    fn drop(&mut self) {
        self.0.sub(self.1);
    }
}

pub async fn wrap_future_keep_guards<F: Future>(
    future: F,
    _guards: Vec<EntranceGuard>,
) -> F::Output {
    future.await
}

#[derive(Default)]
pub struct Registry {
    counters: RwLock<HashMap<&'static str, &'static Counter>>,
}

impl Registry {
    pub fn global() -> &'static Self {
        static REGISTRY: Lazy<Registry> = Lazy::new(Registry::default);
        &REGISTRY
    }

    pub fn register_counter(&self, counter: &'static Counter) {
        if self
            .counters
            .write()
            .unwrap()
            .insert(counter.name, counter)
            .is_some()
        {
            panic!("Counter {} is duplicated", counter.name)
        }
    }

    pub fn counters(&self) -> HashMap<&'static str, &'static Counter> {
        self.counters.read().unwrap().clone()
    }

    pub fn reset(&self) {
        for counter in self.counters.read().unwrap().values() {
            counter.inner().store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn counters_test() {
        static COUNTER1: Counter = Counter::new_counter("COUNTER1");
        static COUNTER2: Counter = Counter::new_counter("COUNTER2");
        COUNTER1.increment();
        COUNTER2.add(5);
        let counters = Registry::global().counters();
        assert_eq!(1, counters.get("COUNTER1").unwrap().value());
        assert_eq!(5, counters.get("COUNTER2").unwrap().value());
    }

    #[test]
    fn entrance_test() {
        static COUNTER3: Counter = Counter::new_counter("COUNTER3");
        let guard1 = COUNTER3.entrance_guard(1);
        let counters = Registry::global().counters();
        assert_eq!(1, counters.get("COUNTER3").unwrap().value());
        std::mem::drop(guard1);
        let counters = Registry::global().counters();
        assert_eq!(0, counters.get("COUNTER3").unwrap().value());
    }
}
