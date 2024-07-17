/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::AcquireError;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;

use crate::RendezVousController;
use crate::RendezVousOptions;

/// This RendezVousController is parameterized.
///
/// It allows a fixed number of "free" connections. This is how many requests we allow to exist
/// in flight at any point in time. Batching does not kick in until these are exhausted.
///
/// Further parameters define what we do when these are exhausted:
/// - max_threshold: number of keys after which we'll dispatch a full-size batch.
/// - max_delay: controls how long we wait before dispatching a small batch.
///
/// Note that if a batch departs when either of those criteria are met, it will not count against
/// the count of free connections: free connections are just connections not subject to batching,
/// but once batching kicks in there is no limit to how many batches can be in flight concurrently
/// (though unless we receive infinite requests the concurrency will tend to approach the free
/// connection count).
pub struct ConfigurableRendezVousController {
    semaphore: Arc<Semaphore>,
    max_delay: Duration,
    max_threshold: usize,
}

impl ConfigurableRendezVousController {
    pub fn new(opts: RendezVousOptions) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(opts.free_connections)),
            max_delay: opts.max_delay,
            max_threshold: opts.max_threshold,
        }
    }
}

#[async_trait::async_trait]
impl RendezVousController for ConfigurableRendezVousController {
    // NOTE: We don't actually care about AcquireError here, since that can only happen when the
    // Semaphore is closed, but we don't close it.
    type RendezVousToken = Option<Result<OwnedSemaphorePermit, AcquireError>>;

    /// Wait for the configured dispatch delay.
    async fn wait_for_dispatch(&self) -> Self::RendezVousToken {
        tokio::time::timeout(self.max_delay, self.semaphore.clone().acquire_owned())
            .await
            .ok()
    }

    fn early_dispatch_threshold(&self) -> usize {
        self.max_threshold
    }
}
