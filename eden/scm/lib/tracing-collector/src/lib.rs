/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! See [`TracingCollector`] for the main structure.

#![allow(unused_variables)]
#![allow(dead_code)]

pub mod model;
use std::sync::Arc;

pub use model::TracingData;
use parking_lot::Mutex;
use tracing::span::Attributes;
use tracing::span::Record;
use tracing::Event;
use tracing::Id;
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::layer::Layer;

/// A `tokio/tracing` subscriber that collects tracing data to [`TracingData`].
/// [`TracingData`] is independent from `tokio/tracing`. See its docstring for
/// more details.
pub struct TracingCollector {
    data: Arc<Mutex<TracingData>>,
}

impl TracingCollector {
    pub fn new(data: Arc<Mutex<TracingData>>) -> Self {
        Self { data }
    }
}

impl<S: Subscriber> Layer<S> for TracingCollector {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let callsite_id = attrs.metadata().callsite();
        let mut data = self.data.lock();
        let count = data.callsite_entered.entry(callsite_id).or_default();
        *count += 1;
        if *count < data.max_span_ref_count {
            let espan_id = data.new_span(attrs);
            data.insert_id_mapping(id, espan_id);
        }
    }

    fn on_record(&self, span_id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
        let mut data = self.data.lock();
        if let Some(espan_id) = data.get_espan_id_from_trace(span_id) {
            data.record(&espan_id.into(), values);
        }
    }

    fn on_follows_from(&self, span_id: &Id, follows: &Id, _ctx: Context<'_, S>) {
        let mut data = self.data.lock();
        if let Some(espan_id) = data.get_espan_id_from_trace(span_id) {
            if let Some(follows_espan_id) = data.get_espan_id_from_trace(follows) {
                data.record_follows_from(&espan_id.into(), &follows_espan_id.into());
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let callsite_id = event.metadata().callsite();
        let mut data = self.data.lock();
        let count = data.callsite_entered.entry(callsite_id).or_default();
        *count += 1;
        if *count < data.max_span_ref_count {
            data.event(event)
        }
    }

    fn on_enter(&self, span_id: &Id, _ctx: Context<'_, S>) {
        let mut data = self.data.lock();
        if let Some(espan_id) = data.get_espan_id_from_trace(span_id) {
            data.enter(&espan_id.into());
        }
    }

    fn on_exit(&self, span_id: &Id, _ctx: Context<'_, S>) {
        let mut data = self.data.lock();
        if let Some(espan_id) = data.get_espan_id_from_trace(span_id) {
            data.exit(&espan_id.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use tracing::instrument;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    use super::*;

    #[instrument]
    fn fib(x: u32) -> u32 {
        match x {
            0 | 1 => 1,
            2 => 2,
            _ => fib(x - 1) + fib(x - 2),
        }
    }

    #[test]
    fn test_instrument() {
        let data = TracingData::new_for_test();
        let data = Arc::new(Mutex::new(data));
        let collector = TracingCollector::new(data.clone());

        tracing::subscriber::with_default(Registry::default().with(collector), || fib(5));

        let mut data = data.lock();
        data.fixup_module_lines_for_tests();

        assert_eq!(
            data.ascii(&Default::default()),
            r#"Process _ Thread _ Start Time _:
Start Dur.ms | Name               Source
    2    +34 | fib                <mod> line <line>
             | - x = 5            :
    4    +18  \ fib               <mod> line <line>
               | - x = 4          :
    6    +10   \ fib              <mod> line <line>
                | - x = 3         :
    8     +2    \ fib             <mod> line <line>
                 | - x = 2        :
   12     +2    \ fib             <mod> line <line>
                 | - x = 1        :
   18     +2   \ fib              <mod> line <line>
                | - x = 2         :
   24    +10  \ fib               <mod> line <line>
               | - x = 3          :
   26     +2   \ fib              <mod> line <line>
                | - x = 2         :
   30     +2   \ fib              <mod> line <line>
                | - x = 1         :

"#
        );
    }

    #[test]
    fn test_multi_threads() {
        let data = TracingData::new_for_test();
        let data = Arc::new(Mutex::new(data));
        let collector = TracingCollector::new(data.clone());

        tracing::subscriber::with_default(Registry::default().with(collector), || fib(0));
        let cloned = data.clone();
        let thread = std::thread::spawn(|| {
            let collector = TracingCollector::new(cloned);
            tracing::subscriber::with_default(Registry::default().with(collector), || fib(3));
        });
        thread.join().unwrap();

        let cloned = data.clone();
        let thread = std::thread::spawn(|| {
            let collector = TracingCollector::new(cloned);
            tracing::subscriber::with_default(Registry::default().with(collector), || fib(2));
        });
        thread.join().unwrap();
        data.lock().fixup_module_lines_for_tests();

        assert_eq!(
            data.lock().ascii(&Default::default()),
            r#"Process _ Thread _ Start Time _:
Start Dur.ms | Name               Source
    2     +2 | fib                <mod> line <line>
             | - x = 0            :

Process _ Thread _ Start Time _:
Start Dur.ms | Name               Source
    6    +10 | fib                <mod> line <line>
             | - x = 3            :
    8     +2  \ fib               <mod> line <line>
               | - x = 2          :
   12     +2  \ fib               <mod> line <line>
               | - x = 1          :

Process _ Thread _ Start Time _:
Start Dur.ms | Name               Source
   18     +2 | fib                <mod> line <line>
             | - x = 2            :

"#
        );
    }

    #[test]
    fn test_span_count_limit() {
        let mut data = TracingData::new_for_test();
        data.max_span_ref_count = 5;
        let data = Arc::new(Mutex::new(data));
        let collector = TracingCollector::new(data.clone());

        tracing::subscriber::with_default(Registry::default().with(collector), || fib(10));
        data.lock().fixup_module_lines_for_tests();

        // fib(6) ... are not logged.
        assert_eq!(
            data.lock().ascii(&Default::default()),
            r#"Process _ Thread _ Start Time _:
Start Dur.ms | Name               Source
    2    +14 | fib                <mod> line <line>
             | - x = 10           :
    4    +10 | fib                <mod> line <line>
             | - x = 9            :
    6     +6 | fib                <mod> line <line>
             | - x = 8            :
    8     +2 | fib                <mod> line <line>
             | - x = 7            :

"#
        );
    }

    #[test]
    fn test_log_count_limit() {
        let mut data = TracingData::new_for_test();
        data.max_span_ref_count = 5;
        let data = Arc::new(Mutex::new(data));
        let collector = TracingCollector::new(data.clone());

        let counts = tracing::subscriber::with_default(Registry::default().with(collector), || {
            (0..10)
                .map(|_| {
                    tracing::info!("log something");
                    data.lock().eventus_len_for_tests()
                })
                .collect::<Vec<usize>>()
        });

        // Repetitive logs are ignored.
        assert_eq!(counts, [1, 2, 3, 4, 4, 4, 4, 4, 4, 4]);
    }
}
