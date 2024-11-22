/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/store/hg/SaplingBackingStore.h"

#include <algorithm>
#include <chrono>
#include <thread>
#include <utility>
#include <variant>

#include <re2/re2.h>

#include <folly/Executor.h>
#include <folly/Range.h>
#include <folly/String.h>
#include <folly/executors/CPUThreadPoolExecutor.h>
#include <folly/executors/task_queue/UnboundedBlockingQueue.h>
#include <folly/executors/thread_factory/InitThreadFactory.h>
#include <folly/futures/Future.h>
#include <folly/logging/xlog.h>
#include <folly/portability/GFlags.h>
#include <folly/system/ThreadName.h>

#include "eden/common/telemetry/RequestMetricsScope.h"
#include "eden/common/telemetry/StructuredLogger.h"
#include "eden/common/utils/Bug.h"
#include "eden/common/utils/EnumValue.h"
#include "eden/common/utils/FaultInjector.h"
#include "eden/common/utils/PathFuncs.h"
#include "eden/common/utils/Throw.h"
#include "eden/common/utils/UnboundedQueueExecutor.h"
#include "eden/fs/config/ReloadableConfig.h"
#include "eden/fs/model/Blob.h"
#include "eden/fs/model/BlobAuxData.h"
#include "eden/fs/model/TreeAuxData.h"
#include "eden/fs/service/ThriftUtil.h"
#include "eden/fs/store/BackingStoreLogger.h"
#include "eden/fs/store/LocalStore.h"
#include "eden/fs/store/ObjectFetchContext.h"
#include "eden/fs/store/StoreResult.h"
#include "eden/fs/store/hg/HgProxyHash.h"
#include "eden/fs/store/hg/SaplingImportRequest.h"
#include "eden/fs/telemetry/EdenStats.h"
#include "eden/fs/telemetry/LogEvent.h"
#include "eden/fs/utils/StaticAssert.h"
#ifdef EDEN_HAVE_SERVER_OBSERVER
#include "common/fb303/cpp/ThreadPoolExecutorCounters.h" // @manual
#endif
#include "common/network/Hostname.h"

DEFINE_bool(
    hg_fetch_missing_trees,
    true,
    "Set this parameter to \"no\" to disable fetching missing treemanifest "
    "trees from the remote mercurial server.  This is generally only useful "
    "for testing/debugging purposes");

namespace facebook::eden {

namespace {
// 100,000 hg object fetches in a short term is plausible.
constexpr size_t kTraceBusCapacity = 100000;
static_assert(CheckSize<HgImportTraceEvent, 72>());
// TraceBus is double-buffered, so the following capacity should be doubled.
// 10 MB overhead per backing repo is tolerable.
static_assert(
    CheckEqual<7200000, kTraceBusCapacity * sizeof(HgImportTraceEvent)>());
ObjectId hashFromRootId(const RootId& root) {
  return ObjectId::fromHex(root.value());
}

/**
 * Thread factory that sets thread name and initializes a thread local
 * Sapling retry state.
 */
class SaplingRetryThreadFactory : public folly::InitThreadFactory {
 public:
  SaplingRetryThreadFactory(
      AbsolutePathPiece repository,
      EdenStatsPtr stats,
      std::shared_ptr<StructuredLogger> logger)
      : folly::InitThreadFactory(
            std::make_shared<folly::NamedThreadFactory>("SaplingRetry"),
            [repository = AbsolutePath{repository},
             stats = std::move(stats),
             logger] {},
            [] {}) {}
};

sapling::SaplingNativeBackingStoreOptions computeSaplingOptions() {
  sapling::SaplingNativeBackingStoreOptions options{};
  options.allow_retries = false;
  return options;
}

sapling::SaplingNativeBackingStoreOptions computeTestSaplingOptions() {
  sapling::SaplingNativeBackingStoreOptions options{};
  options.allow_retries = false;
  return options;
}

std::unique_ptr<SaplingBackingStoreOptions> computeRuntimeOptions(
    std::unique_ptr<SaplingBackingStoreOptions> options) {
  // No options are currently set. See D64436672 for an example on how to add
  // this back if the mechanism is needed in the future.
  return options;
}

TreeEntryType fromRawTreeEntryType(sapling::TreeEntryType type) {
  switch (type) {
    case sapling::TreeEntryType::RegularFile:
      return TreeEntryType::REGULAR_FILE;
    case sapling::TreeEntryType::Tree:
      return TreeEntryType::TREE;
    case sapling::TreeEntryType::ExecutableFile:
      return TreeEntryType::EXECUTABLE_FILE;
    case sapling::TreeEntryType::Symlink:
      return TreeEntryType::SYMLINK;
  }
  EDEN_BUG() << "unknown tree entry type " << static_cast<uint32_t>(type)
             << " loaded from data store";
}

Tree::value_type fromRawTreeEntry(
    sapling::TreeEntry entry,
    RelativePathPiece path,
    HgObjectIdFormat hgObjectIdFormat) {
  std::optional<uint64_t> size;
  std::optional<Hash20> contentSha1;
  std::optional<Hash32> contentBlake3;

  if (entry.has_size) {
    size = entry.size;
  }

  if (entry.has_sha1) {
    contentSha1.emplace(Hash20(std::move(entry.content_sha1)));
  }

  if (entry.has_blake3) {
    contentBlake3.emplace(Hash32(std::move(entry.content_blake3)));
  }

  auto name = PathComponent(folly::StringPiece{
      folly::ByteRange{entry.name.data(), entry.name.size()}});
  Hash20 hash(std::move(entry.hash));

  auto fullPath = path + name;
  auto proxyHash = HgProxyHash::store(fullPath, hash, hgObjectIdFormat);

  auto treeEntry = TreeEntry{
      proxyHash,
      fromRawTreeEntryType(entry.ttype),
      size,
      contentSha1,
      contentBlake3};
  return {std::move(name), std::move(treeEntry)};
}

TreePtr fromRawTree(
    const sapling::Tree* tree,
    const ObjectId& edenTreeId,
    RelativePathPiece path,
    HgObjectIdFormat hgObjectIdFormat) {
  Tree::container entries{kPathMapDefaultCaseSensitive};

  entries.reserve(tree->entries.size());
  for (uintptr_t i = 0; i < tree->entries.size(); i++) {
    try {
      auto entry = fromRawTreeEntry(tree->entries[i], path, hgObjectIdFormat);
      entries.emplace(entry.first, std::move(entry.second));
    } catch (const PathComponentContainsDirectorySeparator& ex) {
      XLOGF(WARN, "Ignoring directory entry: {}", ex.what());
    }
  }
  return std::make_shared<TreePtr::element_type>(
      std::move(entries), edenTreeId);
}

std::unique_ptr<folly::Executor> makeRetryThreadPool(
    AbsolutePathPiece repository,
    const EdenStatsPtr& stats,
    std::shared_ptr<StructuredLogger> structuredLogger,
    uint8_t num_threads) {
  std::unique_ptr<folly::CPUThreadPoolExecutor> retryThreadPool =
      std::make_unique<folly::CPUThreadPoolExecutor>(
          num_threads,
          /* Eden performance will degrade when, for example, a status operation
           * causes a large number of import requests to be scheduled before a
           * lightweight operation needs to check the RocksDB cache. In that
           * case, the RocksDB threads can end up all busy inserting work into
           * the retry queue, preventing future requests that would hit cache
           * from succeeding.
           *
           * Thus, make the retry queue unbounded.
           *
           * In the long term, we'll want a more comprehensive approach to
           * bounding the parallelism of scheduled work.
           */
          folly::CPUThreadPoolExecutor::makeDefaultQueue(),
          std::make_shared<SaplingRetryThreadFactory>(
              repository, stats.copy(), structuredLogger));
#ifdef EDEN_HAVE_SERVER_OBSERVER
  facebook::fb303::installThreadPoolExecutorCounters("", *retryThreadPool);
#endif
  return retryThreadPool;
}

} // namespace

HgImportTraceEvent::HgImportTraceEvent(
    uint64_t unique,
    EventType eventType,
    ResourceType resourceType,
    const HgProxyHash& proxyHash,
    ImportPriority::Class priority,
    ObjectFetchContext::Cause cause,
    OptionalProcessId pid,
    std::optional<ObjectFetchContext::FetchedSource> fetchedSource)
    : unique{unique},
      manifestNodeId{proxyHash.revHash()},
      eventType{eventType},
      resourceType{resourceType},
      importPriority{priority},
      importCause{cause},
      pid{pid},
      fetchedSource{fetchedSource} {
  auto hgPath = proxyHash.path().view();
  // TODO: If HgProxyHash (and correspondingly ObjectId) used an immutable,
  // refcounted string, we wouldn't need to allocate here.
  path.reset(new char[hgPath.size() + 1]);
  memcpy(path.get(), hgPath.data(), hgPath.size());
  path[hgPath.size()] = 0;
}

SaplingBackingStore::SaplingBackingStore(
    AbsolutePathPiece repository,
    std::shared_ptr<LocalStore> localStore,
    EdenStatsPtr stats,
    UnboundedQueueExecutor* serverThreadPool,
    std::shared_ptr<ReloadableConfig> config,
    std::unique_ptr<SaplingBackingStoreOptions> runtimeOptions,
    std::shared_ptr<StructuredLogger> structuredLogger,
    std::unique_ptr<BackingStoreLogger> logger,
    FaultInjector* FOLLY_NONNULL faultInjector)
    : localStore_(std::move(localStore)),
      stats_(stats.copy()),
      retryThreadPool_(makeRetryThreadPool(
          repository,
          stats,
          structuredLogger,
          config->getEdenConfig()->hgNumRetryThreads.getValue())),
      config_(config),
      serverThreadPool_(serverThreadPool),
      queue_(std::move(config)),
      structuredLogger_{std::move(structuredLogger)},
      logger_(std::move(logger)),
      faultInjector_{*faultInjector},
      localStoreCachingPolicy_{constructLocalStoreCachingPolicy()},
      runtimeOptions_(computeRuntimeOptions(std::move(runtimeOptions))),
      activityBuffer_{
          config_->getEdenConfig()->hgActivityBufferSize.getValue()},
      traceBus_{TraceBus<HgImportTraceEvent>::create(
          "hg",
          config_->getEdenConfig()->HgTraceBusCapacity.getValue())},
      store_{repository.view(), computeSaplingOptions()} {
  uint8_t numberThreads =
      config_->getEdenConfig()->numBackingstoreThreads.getValue();
  if (!numberThreads) {
    XLOG(
        WARN,
        "SaplingBackingStore configured to use 0 threads. Invalid, using one thread instead");
    numberThreads = 1;
  }
  threads_.reserve(numberThreads);
  for (uint16_t i = 0; i < numberThreads; i++) {
    threads_.emplace_back(&SaplingBackingStore::processRequest, this);
  }

  hgTraceHandle_ = traceBus_->subscribeFunction(
      folly::to<std::string>("hg-activitybuffer-", getRepoName().value_or("")),
      [this](const HgImportTraceEvent& event) { this->processHgEvent(event); });

  if (config_->getEdenConfig()->enableOBCOnEden.getValue()) {
    initializeOBCCounters();
  }
}

/**
 * Create an SaplingBackingStore suitable for use in unit tests. It uses an
 * inline executor to process loaded objects rather than the thread pools used
 * in production Eden.
 */
SaplingBackingStore::SaplingBackingStore(
    AbsolutePathPiece repository,
    std::shared_ptr<LocalStore> localStore,
    EdenStatsPtr stats,
    std::shared_ptr<ReloadableConfig> config,
    std::unique_ptr<SaplingBackingStoreOptions> runtimeOptions,
    std::shared_ptr<StructuredLogger> structuredLogger,
    std::unique_ptr<BackingStoreLogger> logger,
    FaultInjector* FOLLY_NONNULL faultInjector)
    : localStore_(std::move(localStore)),
      stats_(std::move(stats)),
      retryThreadPool_{std::make_unique<folly::InlineExecutor>()},
      config_(config),
      serverThreadPool_(retryThreadPool_.get()),
      queue_(std::move(config)),
      structuredLogger_{std::move(structuredLogger)},
      logger_(std::move(logger)),
      faultInjector_{*faultInjector},
      localStoreCachingPolicy_{constructLocalStoreCachingPolicy()},
      runtimeOptions_(std::move(runtimeOptions)),
      activityBuffer_{
          config_->getEdenConfig()->hgActivityBufferSize.getValue()},
      traceBus_{TraceBus<HgImportTraceEvent>::create(
          "hg",
          config_->getEdenConfig()->HgTraceBusCapacity.getValue())},
      store_{repository.view(), computeTestSaplingOptions()} {
  uint8_t numberThreads =
      config_->getEdenConfig()->numBackingstoreThreads.getValue();
  if (!numberThreads) {
    XLOG(
        WARN,
        "SaplingBackingStore configured to use 0 threads. Invalid, using one thread instead");
    numberThreads = 1;
  }
  threads_.reserve(numberThreads);
  for (uint16_t i = 0; i < numberThreads; i++) {
    threads_.emplace_back(&SaplingBackingStore::processRequest, this);
  }

  hgTraceHandle_ = traceBus_->subscribeFunction(
      folly::to<std::string>("hg-activitybuffer-", getRepoName().value_or("")),
      [this](const HgImportTraceEvent& event) { this->processHgEvent(event); });

  if (config_->getEdenConfig()->enableOBCOnEden.getValue()) {
    initializeOBCCounters();
  }
}

SaplingBackingStore::~SaplingBackingStore() {
  queue_.stop();
  for (auto& thread : threads_) {
    thread.join();
  }
}

void SaplingBackingStore::initializeOBCCounters() {
  std::string repoName = store_.getRepoName().data();
  // Get the hostname without the ".facebook.com" suffix
  auto hostname = facebook::network::getLocalHost(/*stripFbDomain=*/true);
  getBlobPerRepoLatencies_ = monitoring::OBCPxx(
      monitoring::OdsCategoryId::ODS_EDEN,
      fmt::format("eden.store.sapling.fetch_blob_{}_us", repoName),
      {hostname});
  getTreePerRepoLatencies_ = monitoring::OBCPxx(
      monitoring::OdsCategoryId::ODS_EDEN,
      fmt::format("eden.store.sapling.fetch_tree_{}_us", repoName),
      {hostname});
  isOBCEnabled_ = true;
}

BackingStore::LocalStoreCachingPolicy
SaplingBackingStore::constructLocalStoreCachingPolicy() {
  bool shouldCacheTrees =
      config_->getEdenConfig()->hgEnableTreeLocalStoreCaching.getValue();
  bool shouldCacheBlobs =
      config_->getEdenConfig()->hgEnableBlobLocalStoreCaching.getValue();
  bool shouldCacheBlobAuxData =
      config_->getEdenConfig()->hgEnableBlobMetaLocalStoreCaching.getValue();

  using PolicyType =
      std::underlying_type_t<BackingStore::LocalStoreCachingPolicy>;
  PolicyType result =
      static_cast<PolicyType>(BackingStore::LocalStoreCachingPolicy::NoCaching);

  if (shouldCacheTrees) {
    result |=
        static_cast<PolicyType>(BackingStore::LocalStoreCachingPolicy::Trees);
  }

  if (shouldCacheBlobs) {
    result |=
        static_cast<PolicyType>(BackingStore::LocalStoreCachingPolicy::Blobs);
  }

  if (shouldCacheBlobAuxData) {
    result |= static_cast<PolicyType>(
        BackingStore::LocalStoreCachingPolicy::BlobAuxData);
  }
  return static_cast<BackingStore::LocalStoreCachingPolicy>(result);
}

void SaplingBackingStore::processHgEvent(const HgImportTraceEvent& event) {
  switch (event.eventType) {
    case HgImportTraceEvent::QUEUE:
      // Create a new queued event
    case HgImportTraceEvent::START:
      // Override the queued event with start event
      outstandingHgEvents_.wlock()->insert_or_assign(event.unique, event);
      break;
    case HgImportTraceEvent::FINISH:
      outstandingHgEvents_.wlock()->erase(event.unique);
      break;
    default:
      EDEN_BUG() << "Unknown Hg trace event type: "
                 << enumValue(event.eventType);
  }
  activityBuffer_.addEvent(event);
}

void SaplingBackingStore::setPrefetchBlobCounters(
    ObjectFetchContextPtr context,
    ObjectFetchContext::FetchedSource fetchedSource,
    ObjectFetchContext::FetchResult fetchResult,
    folly::stop_watch<std::chrono::milliseconds> watch) {
  if (fetchResult == ObjectFetchContext::FetchResult::Failure) {
    stats_->increment(&SaplingBackingStoreStats::prefetchBlobFailure);
    return;
  }
  stats_->addDuration(&SaplingBackingStoreStats::prefetchBlob, watch.elapsed());

  if (fetchResult == ObjectFetchContext::FetchResult::Success) {
    stats_->increment(&SaplingBackingStoreStats::prefetchBlobSuccess);
  } else if (fetchResult == ObjectFetchContext::FetchResult::SuccessInRetry) {
    stats_->increment(&SaplingBackingStoreStats::prefetchBlobRetrySuccess);
  } else {
    EDEN_BUG() << "Unknown fetch request result: " << enumValue(fetchResult);
  }

  context->setFetchedSource(
      fetchedSource,
      ObjectFetchContext::ObjectType::PrefetchBlob,
      stats_.copy());
}

void SaplingBackingStore::setFetchBlobCounters(
    ObjectFetchContextPtr context,
    ObjectFetchContext::FetchedSource fetchedSource,
    ObjectFetchContext::FetchResult fetchResult,
    folly::stop_watch<std::chrono::milliseconds> watch) {
  if (fetchResult == ObjectFetchContext::FetchResult::Failure) {
    stats_->increment(&SaplingBackingStoreStats::fetchBlobFailure);
    if (store_.dogfoodingHost()) {
      stats_->increment(&SaplingBackingStoreStats::fetchBlobFailureDogfooding);
    }
    return;
  }

  if (isOBCEnabled_) {
    getBlobPerRepoLatencies_ += watch.elapsed().count();
  } else {
    stats_->addDuration(&SaplingBackingStoreStats::fetchBlob, watch.elapsed());
  }

  if (fetchResult == ObjectFetchContext::FetchResult::Success) {
    stats_->increment(&SaplingBackingStoreStats::fetchBlobSuccess);
  } else if (fetchResult == ObjectFetchContext::FetchResult::SuccessInRetry) {
    stats_->increment(&SaplingBackingStoreStats::fetchBlobRetrySuccess);
  } else {
    EDEN_BUG() << "Unknown fetch request result: " << enumValue(fetchResult);
  }

  context->setFetchedSource(
      fetchedSource, ObjectFetchContext::ObjectType::Blob, stats_.copy());

  if (store_.dogfoodingHost()) {
    stats_->increment(&SaplingBackingStoreStats::fetchBlobSuccessDogfooding);
  }
}

void SaplingBackingStore::setBlobCounters(
    ObjectFetchContextPtr context,
    SaplingImportRequest::FetchType fetchType,
    ObjectFetchContext::FetchedSource fetchedSource,
    ObjectFetchContext::FetchResult fetchResult,
    folly::stop_watch<std::chrono::milliseconds> watch) {
  switch (fetchType) {
    case SaplingImportRequest::FetchType::Prefetch:
      setPrefetchBlobCounters(
          context.copy(), fetchedSource, fetchResult, watch);
      break;

    case SaplingImportRequest::FetchType::Fetch:
      setFetchBlobCounters(context.copy(), fetchedSource, fetchResult, watch);
      break;
  }
}

void SaplingBackingStore::processBlobImportRequests(
    std::vector<std::shared_ptr<SaplingImportRequest>>&& requests) {
  folly::stop_watch<std::chrono::milliseconds> watch;

  XLOGF(DBG4, "Processing blob import batch size={}", requests.size());

  for (auto& request : requests) {
    auto* blobImport = request->getRequest<SaplingImportRequest::BlobImport>();

    // TODO: We could reduce the number of lock acquisitions by adding a batch
    // publish method.
    traceBus_->publish(HgImportTraceEvent::start(
        request->getUnique(),
        HgImportTraceEvent::BLOB,
        blobImport->proxyHash,
        request->getPriority().getClass(),
        request->getCause(),
        request->getPid()));

    XLOGF(DBG4, "Processing blob request for {}", blobImport->hash);
  }

  std::vector<std::shared_ptr<SaplingImportRequest>> retryRequest;
  retryRequest.reserve(requests.size());
  if (config_->getEdenConfig()->allowRemoteGetBatch.getValue()) {
    getBlobBatch(requests, sapling::FetchMode::AllowRemote);
    retryRequest = std::move(requests);
  } else {
    getBlobBatch(requests, sapling::FetchMode::LocalOnly);

    for (auto& request : requests) {
      auto* promise = request->getPromise<BlobPtr>();
      if (promise->isFulfilled()) {
        XLOGF(
            DBG4,
            "Blob found in Sapling local for {}",
            request->getRequest<SaplingImportRequest::BlobImport>()->hash);
        setBlobCounters(
            request->getContext().copy(),
            request->getFetchType(),
            ObjectFetchContext::FetchedSource::Local,
            ObjectFetchContext::FetchResult::Success,
            watch);
      } else {
        retryRequest.emplace_back(std::move(request));
      }
    }

    getBlobBatch(retryRequest, sapling::FetchMode::RemoteOnly);
  }

  {
    std::vector<folly::SemiFuture<folly::Unit>> futures;
    futures.reserve(retryRequest.size());

    for (auto& request : retryRequest) {
      auto* promise = request->getPromise<BlobPtr>();
      if (promise->isFulfilled()) {
        setBlobCounters(
            request->getContext().copy(),
            request->getFetchType(),
            config_->getEdenConfig()->allowRemoteGetBatch.getValue()
                ? ObjectFetchContext::FetchedSource::Unknown
                : ObjectFetchContext::FetchedSource::Remote,
            ObjectFetchContext::FetchResult::Success,
            watch);
        continue;
      }
      // The blobs were either not found locally, or, when EdenAPI is enabled,
      // not found on the server. Let's retry to import the blob
      // Note: we don't pass request to this function  to avoid making copies
      // of the shared ptr (which requires an atomic instruction every time
      // the refcount changes)
      auto fetchSemiFuture = retryGetBlob(
          request->getRequest<SaplingImportRequest::BlobImport>()->proxyHash,
          request->getContext().copy(),
          request->getFetchType(),
          watch);
      futures.emplace_back(
          std::move(fetchSemiFuture)
              .defer([request = std::move(request),
                      stats = stats_.copy()](auto&& result) mutable {
                XLOGF(
                    DBG4,
                    "Imported blob from HgImporter for {}",
                    request->getRequest<SaplingImportRequest::BlobImport>()
                        ->hash);
                request
                    ->getPromise<SaplingImportRequest::BlobImport::Response>()
                    ->setTry(std::forward<decltype(result)>(result));
              }));
    }

    folly::collectAll(futures).wait();
  }
}

folly::SemiFuture<BlobPtr> SaplingBackingStore::retryGetBlob(
    HgProxyHash hgInfo,
    ObjectFetchContextPtr context,
    const SaplingImportRequest::FetchType fetch_type,
    folly::stop_watch<std::chrono::milliseconds> watch) {
  return folly::via(
      retryThreadPool_.get(),
      [this,
       hgInfo = std::move(hgInfo),
       context = context.copy(),
       fetch_type,
       watch] {
        std::unique_ptr<RequestMetricsScope> queueTracker;
        switch (fetch_type) {
          case SaplingImportRequest::FetchType::Fetch:
            queueTracker = std::make_unique<RequestMetricsScope>(
                &this->liveImportBlobWatches_);
            break;
          case SaplingImportRequest::FetchType::Prefetch:
            queueTracker = std::make_unique<RequestMetricsScope>(
                &this->liveImportPrefetchWatches_);
            break;
        }

        // NOTE: In the future we plan to update
        // SaplingNativeBackingStore to provide and
        // asynchronous interface enabling us to perform our retries
        // there. In the meantime we use retryThreadPool_ for these
        // longer-running retry requests to avoid starving
        // serverThreadPool_.

        // Flush (and refresh) SaplingNativeBackingStore to ensure all
        // data is written and to rescan pack files or local indexes
        flush();

        // Retry using datapackStore (SaplingNativeBackingStore).
        auto result = folly::makeFuture<BlobPtr>(BlobPtr{nullptr});

        auto fetch_mode =
            config_->getEdenConfig()->allowRemoteGetBatch.getValue()
            ? sapling::FetchMode::AllowRemote
            : sapling::FetchMode::LocalOnly;
        auto blob = getBlobFromBackingStore(hgInfo, fetch_mode);
        if (!blob.hasValue() && fetch_mode == sapling::FetchMode::LocalOnly) {
          // Retry using remote
          fetch_mode = sapling::FetchMode::RemoteOnly;
          blob = getBlobFromBackingStore(hgInfo, fetch_mode);
        }
        auto fetched_source = ObjectFetchContext::FetchedSource::Unknown;
        switch (fetch_mode) {
          case sapling::FetchMode::LocalOnly:
            fetched_source = ObjectFetchContext::FetchedSource::Local;
            break;
          case sapling::FetchMode::RemoteOnly:
            fetched_source = ObjectFetchContext::FetchedSource::Remote;
            break;
          case sapling::FetchMode::AllowRemote:
          case sapling::FetchMode::AllowRemotePrefetch:
            fetched_source = ObjectFetchContext::FetchedSource::Unknown;
            break;
        }

        if (blob.hasValue()) {
          setBlobCounters(
              context.copy(),
              fetch_type,
              fetched_source,
              ObjectFetchContext::FetchResult::SuccessInRetry,
              watch);
          result = blob.value();
        } else {
          // Record miss and return error
          if (structuredLogger_) {
            structuredLogger_->logEvent(FetchMiss{
                store_.getRepoName(),
                FetchMiss::Blob,
                blob.exception().what().toStdString(),
                true, // isRetry
                store_.dogfoodingHost()});
          }
          setBlobCounters(
              context.copy(),
              fetch_type,
              fetched_source,
              ObjectFetchContext::FetchResult::Failure,
              watch);
          auto ew = folly::exception_wrapper{blob.exception()};
          result = folly::makeFuture<BlobPtr>(std::move(ew));
        }
        return result;
      });
}

void SaplingBackingStore::getBlobBatch(
    const ImportRequestsList& importRequests,
    sapling::FetchMode fetchMode) {
  auto preparedRequests = prepareRequests<SaplingImportRequest::BlobImport>(
      importRequests, SaplingImportObject::BLOB);
  auto importRequestsMap = std::move(preparedRequests.first);
  auto requests = std::move(preparedRequests.second);

  store_.getBlobBatch(
      folly::range(requests),
      fetchMode,
      // store_->getBlobBatch is blocking, hence we can take these by reference.
      [&](size_t index, folly::Try<std::unique_ptr<folly::IOBuf>> content) {
        if (content.hasException()) {
          XLOGF(
              DBG4,
              "Failed to import node={} from EdenAPI (batch {}/{}): {}",
              folly::hexlify(requests[index].node),
              index,
              requests.size(),
              content.exception().what().toStdString());
          return;
        } else {
          XLOGF(
              DBG4,
              "Imported node={} from EdenAPI (batch: {}/{})",
              folly::hexlify(requests[index].node),
              index,
              requests.size());
        }

        const auto& nodeId = requests[index].node;
        XLOGF(DBG9, "Imported Blob node={}", folly::hexlify(nodeId));
        auto& [importRequestList, watch] = importRequestsMap[nodeId];
        auto result = content.hasException()
            ? folly::Try<BlobPtr>{content.exception()}
            : folly::Try{
                  std::make_shared<BlobPtr::element_type>(*content.value())};
        for (auto& importRequest : importRequestList) {
          importRequest->getPromise<BlobPtr>()->setWith(
              [&]() -> folly::Try<BlobPtr> { return result; });
        }

        // Make sure that we're stopping this watch.
        watch.reset();
      });
}

folly::Try<BlobPtr> SaplingBackingStore::getBlobFromBackingStore(
    const HgProxyHash& hgInfo,
    sapling::FetchMode fetchMode) {
  auto blob = store_.getBlob(hgInfo.byteHash(), fetchMode);

  using GetBlobResult = folly::Try<BlobPtr>;

  if (blob.hasValue()) {
    return GetBlobResult{
        std::make_shared<BlobPtr::element_type>(std::move(*blob.value()))};
  } else {
    return GetBlobResult{blob.exception()};
  }
}

void SaplingBackingStore::processTreeImportRequests(
    std::vector<std::shared_ptr<SaplingImportRequest>>&& requests) {
  folly::stop_watch<std::chrono::milliseconds> watch;

  for (auto& request : requests) {
    auto* treeImport = request->getRequest<SaplingImportRequest::TreeImport>();

    // TODO: We could reduce the number of lock acquisitions by adding a batch
    // publish method.
    traceBus_->publish(HgImportTraceEvent::start(
        request->getUnique(),
        HgImportTraceEvent::TREE,
        treeImport->proxyHash,
        request->getPriority().getClass(),
        request->getCause(),
        request->getPid()));

    XLOGF(DBG4, "Processing tree request for {}", treeImport->hash);
  }

  std::vector<std::shared_ptr<SaplingImportRequest>> retryRequest;
  retryRequest.reserve(requests.size());
  if (config_->getEdenConfig()->allowRemoteGetBatch.getValue()) {
    getTreeBatch(requests, sapling::FetchMode::AllowRemote);
    retryRequest = std::move(requests);
  } else {
    getTreeBatch(requests, sapling::FetchMode::LocalOnly);
    for (auto& request : requests) {
      auto* promise = request->getPromise<TreePtr>();
      if (promise->isFulfilled()) {
        XLOGF(
            DBG4,
            "Tree found in Sapling local for {}",
            request->getRequest<SaplingImportRequest::TreeImport>()->hash);
        request->getContext()->setFetchedSource(
            ObjectFetchContext::FetchedSource::Local,
            ObjectFetchContext::ObjectType::Tree,
            stats_.copy());
        if (isOBCEnabled_) {
          getTreePerRepoLatencies_ += watch.elapsed().count();
        } else {
          stats_->addDuration(
              &SaplingBackingStoreStats::fetchTree, watch.elapsed());
        }
        stats_->increment(&SaplingBackingStoreStats::fetchTreeSuccess);
        if (store_.dogfoodingHost()) {
          stats_->increment(
              &SaplingBackingStoreStats::fetchTreeSuccessDogfooding);
        }
      } else {
        retryRequest.emplace_back(std::move(request));
      }
    }
    getTreeBatch(retryRequest, sapling::FetchMode::RemoteOnly);
  }

  {
    std::vector<folly::SemiFuture<folly::Unit>> futures;
    futures.reserve(retryRequest.size());

    for (auto& request : retryRequest) {
      auto* promise = request->getPromise<TreePtr>();
      if (promise->isFulfilled()) {
        if (!config_->getEdenConfig()->allowRemoteGetBatch.getValue()) {
          XLOGF(
              DBG4,
              "Tree found in Sapling remote for {}",
              request->getRequest<SaplingImportRequest::TreeImport>()->hash);
          request->getContext()->setFetchedSource(
              ObjectFetchContext::FetchedSource::Remote,
              ObjectFetchContext::ObjectType::Tree,
              stats_.copy());
        }
        if (isOBCEnabled_) {
          getTreePerRepoLatencies_ += watch.elapsed().count();
        } else {
          stats_->addDuration(
              &SaplingBackingStoreStats::fetchTree, watch.elapsed());
        }
        stats_->increment(&SaplingBackingStoreStats::fetchTreeSuccess);
        if (store_.dogfoodingHost()) {
          stats_->increment(
              &SaplingBackingStoreStats::fetchTreeSuccessDogfooding);
        }
        continue;
      }

      // The trees were either not found locally, or, when EdenAPI is enabled,
      // not found on the server. Let's retry to import the trees
      stats_->increment(&SaplingBackingStoreStats::fetchTreeFailure);
      if (store_.dogfoodingHost()) {
        stats_->increment(
            &SaplingBackingStoreStats::fetchTreeFailureDogfooding);
      }
      auto* treeImport =
          request->getRequest<SaplingImportRequest::TreeImport>();
      auto treeSemiFuture =
          retryGetTree(
              treeImport->proxyHash
                  .revHash(), // this is really the manifest node
              treeImport->hash,
              treeImport->proxyHash.path(),
              request->getContext().copy(),
              ObjectFetchContext::ObjectType::Tree)
              .semi();
      futures.emplace_back(
          std::move(treeSemiFuture)
              .defer([request = std::move(request),
                      watch,
                      stats = stats_.copy()](auto&& result) mutable {
                XLOGF(
                    DBG4,
                    "Imported tree after retry for {}",
                    request->getRequest<SaplingImportRequest::TreeImport>()
                        ->hash);
                stats->addDuration(
                    &SaplingBackingStoreStats::fetchTree, watch.elapsed());
                request
                    ->getPromise<SaplingImportRequest::TreeImport::Response>()
                    ->setTry(std::forward<decltype(result)>(result));
              }));
    }

    folly::collectAll(futures).wait();
  }
}

void SaplingBackingStore::getTreeBatch(
    const ImportRequestsList& importRequests,
    sapling::FetchMode fetch_mode) {
  auto preparedRequests = prepareRequests<SaplingImportRequest::TreeImport>(
      importRequests, SaplingImportObject::TREE);
  auto importRequestsMap = std::move(preparedRequests.first);
  auto requests = std::move(preparedRequests.second);
  auto hgObjectIdFormat = config_->getEdenConfig()->hgObjectIdFormat.getValue();

  faultInjector_.check("SaplingBackingStore::getTreeBatch", "");
  store_.getTreeBatch(
      folly::range(requests),
      fetch_mode,
      // getTreeBatch is blocking, hence we can take these by
      // reference.
      [&](size_t index,
          folly::Try<std::shared_ptr<sapling::Tree>> content) mutable {
        if (content.hasException()) {
          XLOGF(
              DBG4,
              "Failed to import node={} from EdenAPI (batch tree {}/{}): {}",
              folly::hexlify(requests[index].node),
              index,
              requests.size(),
              content.exception().what().toStdString());
          return;
        } else {
          XLOGF(
              DBG4,
              "Imported node={} from EdenAPI (batch tree: {}/{})",
              folly::hexlify(requests[index].node),
              index,
              requests.size());
        }

        const auto& nodeId = requests[index].node;
        XLOGF(DBG9, "Imported Tree node={}", folly::hexlify(nodeId));
        auto& [importRequestList, watch] = importRequestsMap[nodeId];
        for (auto& importRequest : importRequestList) {
          auto* treeRequest =
              importRequest->getRequest<SaplingImportRequest::TreeImport>();
          importRequest->getPromise<TreePtr>()->setWith(
              [&]() -> folly::Try<TreePtr> {
                if (content.hasException()) {
                  return folly::Try<TreePtr>{content.exception()};
                }
                return folly::Try{fromRawTree(
                    content.value().get(),
                    treeRequest->hash,
                    treeRequest->proxyHash.path(),
                    hgObjectIdFormat)};
              });
        }

        // Make sure that we're stopping this watch.
        watch.reset();
      });
}

template <typename T>
std::pair<
    SaplingBackingStore::ImportRequestsMap,
    std::vector<sapling::SaplingRequest>>
SaplingBackingStore::prepareRequests(
    const ImportRequestsList& importRequests,
    const SaplingImportObject& requestType) {
  // TODO: extract each ClientRequestInfo from importRequests into a
  // sapling::ClientRequestInfo and pass them with the corresponding
  // sapling::NodeId

  // Group requests by proxyHash to ensure no duplicates in fetch request to
  // SaplingNativeBackingStore.
  ImportRequestsMap importRequestsMap;
  for (const auto& importRequest : importRequests) {
    auto nodeId = importRequest->getRequest<T>()->proxyHash.byteHash();

    // Look for and log duplicates.
    const auto& importRequestsEntry = importRequestsMap.find(nodeId);
    if (importRequestsEntry != importRequestsMap.end()) {
      XLOGF(
          DBG9,
          "Duplicate {} fetch request with proxyHash: {}",
          stringOfSaplingImportObject(requestType),
          folly::StringPiece{nodeId});
      auto& importRequestList = importRequestsEntry->second.first;

      // Only look for mismatched requests if logging level is high enough.
      // Make sure this level is the same as the XLOG_IF statement below.
      if (XLOG_IS_ON(DBG9)) {
        // Log requests that do not have the same hash (ObjectId).
        // This happens when two paths (file or directory) have same content.
        for (const auto& priorRequest : importRequestList) {
          XLOGF_IF(
              DBG9,
              UNLIKELY(
                  priorRequest->template getRequest<T>()->hash !=
                  importRequest->getRequest<T>()->hash),
              "{} requests have the same proxyHash (HgProxyHash) but different hash (ObjectId). "
              "This should not happen. Previous request: hash='{}', proxyHash='{}', proxyHash.path='{}'; "
              "current request: hash='{}', proxyHash ='{}', proxyHash.path='{}'.",
              stringOfSaplingImportObject(requestType),
              priorRequest->template getRequest<T>()->hash.asHexString(),
              folly::hexlify(
                  priorRequest->template getRequest<T>()->proxyHash.byteHash()),
              priorRequest->template getRequest<T>()->proxyHash.path(),
              importRequest->getRequest<T>()->hash.asHexString(),
              folly::hexlify(
                  importRequest->getRequest<T>()->proxyHash.byteHash()),
              importRequest->getRequest<T>()->proxyHash.path());
        }
      }

      importRequestList.emplace_back(importRequest);
    } else {
      std::vector<std::shared_ptr<SaplingImportRequest>> requests(
          {importRequest});
      switch (requestType) {
        case SaplingImportObject::TREE:
          importRequestsMap.emplace(
              nodeId, make_pair(requests, &liveBatchedTreeWatches_));
          break;
        case SaplingImportObject::TREE_AUX:
          importRequestsMap.emplace(
              nodeId, make_pair(requests, &liveBatchedTreeAuxWatches_));
          break;
        case SaplingImportObject::BLOB:
          importRequestsMap.emplace(
              nodeId, make_pair(requests, &liveBatchedBlobWatches_));
          break;
        case SaplingImportObject::BLOB_AUX:
          importRequestsMap.emplace(
              nodeId, make_pair(requests, &liveBatchedBlobAuxWatches_));
          break;
        // The following types cannot get here. It is just for completeness
        case SaplingImportObject::BATCHED_TREE:
        case SaplingImportObject::BATCHED_TREE_AUX:
        case SaplingImportObject::BATCHED_BLOB:
        case SaplingImportObject::BATCHED_BLOB_AUX:
        case SaplingImportObject::PREFETCH:
          break;
      }
    }
  }

  // Indexable vector of nodeIds - required by SaplingNativeBackingStore API.
  // With the current implementation, we can't efficiently deduplicate the
  // requests only based on nodeId since multiple requests for the same nodeId
  // can have different FetchCauses, which might trigger different behaviors in
  // the backingstore.
  std::vector<sapling::SaplingRequest> requests;
  for (const auto& importRequestsIdPair : importRequestsMap) {
    // Deduplicate the requests for a given nodeId based on the FetchCause.
    std::set<ObjectFetchContext::Cause> seenCausesForId;
    const ImportRequestsList& importRequestsForId =
        importRequestsIdPair.second.first;
    for (const auto& request : importRequestsForId) {
      if (request &&
          (seenCausesForId.find(request->getCause()) ==
           seenCausesForId.end())) {
        requests.push_back(sapling::SaplingRequest{
            importRequestsIdPair.first, request->getCause()});
        // Mark this cause as seen
        seenCausesForId.insert(request->getCause());
      }
    }
  }

  return std::make_pair(std::move(importRequestsMap), std::move(requests));
}

void SaplingBackingStore::processBlobAuxImportRequests(
    std::vector<std::shared_ptr<SaplingImportRequest>>&& requests) {
  folly::stop_watch<std::chrono::milliseconds> watch;

  for (auto& request : requests) {
    auto* blobAuxImport =
        request->getRequest<SaplingImportRequest::BlobAuxImport>();

    // TODO: We could reduce the number of lock acquisitions by adding a batch
    // publish method.
    traceBus_->publish(HgImportTraceEvent::start(
        request->getUnique(),
        HgImportTraceEvent::BLOB_AUX,
        blobAuxImport->proxyHash,
        request->getPriority().getClass(),
        request->getCause(),
        request->getPid()));

    XLOGF(DBG4, "Processing blob aux request for {}", blobAuxImport->hash);
  }

  std::vector<std::shared_ptr<SaplingImportRequest>> retryRequest;
  retryRequest.reserve(requests.size());
  if (config_->getEdenConfig()->allowRemoteGetBatch.getValue()) {
    getBlobAuxDataBatch(requests, sapling::FetchMode::AllowRemote);
    retryRequest = std::move(requests);
  } else {
    getBlobAuxDataBatch(requests, sapling::FetchMode::LocalOnly);
    for (auto& request : requests) {
      auto* promise = request->getPromise<BlobAuxDataPtr>();
      if (promise->isFulfilled()) {
        XLOGF(
            DBG4,
            "BlobAuxData found in Sapling local for {}",
            request->getRequest<SaplingImportRequest::BlobAuxImport>()->hash);
        request->getContext()->setFetchedSource(
            ObjectFetchContext::FetchedSource::Local,
            ObjectFetchContext::ObjectType::BlobAuxData,
            stats_.copy());
        stats_->addDuration(
            &SaplingBackingStoreStats::fetchBlobAuxData, watch.elapsed());
        stats_->increment(&SaplingBackingStoreStats::fetchBlobAuxDataSuccess);
      } else {
        retryRequest.emplace_back(std::move(request));
      }
    }
    getBlobAuxDataBatch(retryRequest, sapling::FetchMode::RemoteOnly);
  }

  {
    for (auto& request : retryRequest) {
      auto* promise = request->getPromise<BlobAuxDataPtr>();
      if (promise->isFulfilled()) {
        if (!config_->getEdenConfig()->allowRemoteGetBatch.getValue()) {
          XLOGF(
              DBG4,
              "BlobAuxData found in Sapling remote for {}",
              request->getRequest<SaplingImportRequest::BlobAuxImport>()->hash);
          request->getContext()->setFetchedSource(
              ObjectFetchContext::FetchedSource::Remote,
              ObjectFetchContext::ObjectType::BlobAuxData,
              stats_.copy());
        }
        stats_->addDuration(
            &SaplingBackingStoreStats::fetchBlobAuxData, watch.elapsed());
        stats_->increment(&SaplingBackingStoreStats::fetchBlobAuxDataSuccess);
        continue;
      }

      // The code waiting on the promise will fallback to fetching the Blob to
      // compute the blob aux data. We can't trigger a blob fetch here without
      // the risk of running into a deadlock: if all import thread are in this
      // code path, there are no free importer to fetch blobs.
      stats_->increment(&SaplingBackingStoreStats::fetchBlobAuxDataFailure);
      promise->setValue(nullptr);
    }
  }
}

void SaplingBackingStore::processTreeAuxImportRequests(
    std::vector<std::shared_ptr<SaplingImportRequest>>&& requests) {
  folly::stop_watch<std::chrono::milliseconds> watch;

  for (auto& request : requests) {
    auto* treeAuxImport =
        request->getRequest<SaplingImportRequest::TreeAuxImport>();

    // TODO: We could reduce the number of lock acquisitions by adding a batch
    // publish method.
    traceBus_->publish(HgImportTraceEvent::start(
        request->getUnique(),
        HgImportTraceEvent::TREE_AUX,
        treeAuxImport->proxyHash,
        request->getPriority().getClass(),
        request->getCause(),
        request->getPid()));

    XLOGF(DBG4, "Processing tree aux request for {}", treeAuxImport->hash);
  }

  std::vector<std::shared_ptr<SaplingImportRequest>> retryRequest;
  retryRequest.reserve(requests.size());
  if (config_->getEdenConfig()->allowRemoteGetBatch.getValue()) {
    getTreeAuxDataBatch(requests, sapling::FetchMode::AllowRemote);
    retryRequest = std::move(requests);
  } else {
    getTreeAuxDataBatch(requests, sapling::FetchMode::LocalOnly);
    for (auto& request : requests) {
      auto* promise = request->getPromise<TreeAuxDataPtr>();
      if (promise->isFulfilled()) {
        XLOGF(
            DBG4,
            "TreeAuxData found in Sapling local for {}",
            request->getRequest<SaplingImportRequest::TreeAuxImport>()->hash);
        request->getContext()->setFetchedSource(
            ObjectFetchContext::FetchedSource::Local,
            ObjectFetchContext::ObjectType::TreeAuxData,
            stats_.copy());
        stats_->addDuration(
            &SaplingBackingStoreStats::fetchTreeAuxData, watch.elapsed());
        stats_->increment(&SaplingBackingStoreStats::fetchTreeAuxDataSuccess);
      } else {
        retryRequest.emplace_back(std::move(request));
      }
    }
    getTreeAuxDataBatch(retryRequest, sapling::FetchMode::RemoteOnly);
  }

  {
    for (auto& request : retryRequest) {
      auto* promise = request->getPromise<TreeAuxDataPtr>();
      if (promise->isFulfilled()) {
        if (!config_->getEdenConfig()->allowRemoteGetBatch.getValue()) {
          XLOGF(
              DBG4,
              "TreeAuxData found in Sapling remote for {}",
              request->getRequest<SaplingImportRequest::TreeAuxImport>()->hash);
          request->getContext()->setFetchedSource(
              ObjectFetchContext::FetchedSource::Remote,
              ObjectFetchContext::ObjectType::TreeAuxData,
              stats_.copy());
        }
        stats_->addDuration(
            &SaplingBackingStoreStats::fetchTreeAuxData, watch.elapsed());
        stats_->increment(&SaplingBackingStoreStats::fetchTreeAuxDataSuccess);
        continue;
      }

      stats_->increment(&SaplingBackingStoreStats::fetchTreeAuxDataFailure);
      promise->setValue(nullptr);
    }
  }
}

void SaplingBackingStore::getTreeAuxDataBatch(
    const ImportRequestsList& importRequests,
    sapling::FetchMode fetch_mode) {
  auto preparedRequests = prepareRequests<SaplingImportRequest::TreeAuxImport>(
      importRequests, SaplingImportObject::TREE_AUX);
  auto importRequestsMap = std::move(preparedRequests.first);
  auto requests = std::move(preparedRequests.second);

  store_.getTreeAuxDataBatch(
      folly::range(requests),
      fetch_mode,
      // store_.getTreeAuxDataBatch is blocking, hence we can take these by
      // reference.
      [&](size_t index,
          folly::Try<std::shared_ptr<sapling::TreeAuxData>> auxTry) {
        if (auxTry.hasException()) {
          XLOGF(
              DBG6,
              "Failed to import aux data node={} from EdenAPI (batch {}/{}): {}",
              folly::hexlify(requests[index].node),
              index,
              requests.size(),
              auxTry.exception().what().toStdString());
        } else {
          XLOGF(
              DBG6,
              "Imported aux data node={} from EdenAPI (batch: {}/{})",
              folly::hexlify(requests[index].node),
              index,
              requests.size());
        }

        if (auxTry.hasException()) {
          if (structuredLogger_) {
            structuredLogger_->logEvent(FetchMiss{
                store_.getRepoName(),
                FetchMiss::TreeAuxData,
                auxTry.exception().what().toStdString(),
                false, // isRetry
                store_.dogfoodingHost()});
          }

          return;
        }

        const auto& nodeId = requests[index].node;
        XLOGF(DBG9, "Imported TreeAuxData={}", folly::hexlify(nodeId));
        auto& [importRequestList, watch] = importRequestsMap[nodeId];
        folly::Try<TreeAuxDataPtr> result;
        if (auxTry.hasException()) {
          result = folly::Try<TreeAuxDataPtr>{auxTry.exception()};
        } else {
          auto& aux = auxTry.value();
          result = folly::Try{std::make_shared<TreeAuxDataPtr::element_type>(
              Hash32{std::move(aux->digest_hash)}, aux->digest_size)};
        }
        for (auto& importRequest : importRequestList) {
          importRequest->getPromise<TreeAuxDataPtr>()->setWith(
              [&]() -> folly::Try<TreeAuxDataPtr> { return result; });
        }

        // Make sure that we're stopping this watch.
        watch.reset();
      });
}

void SaplingBackingStore::getBlobAuxDataBatch(
    const ImportRequestsList& importRequests,
    sapling::FetchMode fetch_mode) {
  auto preparedRequests = prepareRequests<SaplingImportRequest::BlobAuxImport>(
      importRequests, SaplingImportObject::BLOB_AUX);
  auto importRequestsMap = std::move(preparedRequests.first);
  auto requests = std::move(preparedRequests.second);

  store_.getBlobAuxDataBatch(
      folly::range(requests),
      fetch_mode,
      // store_.getBlobAuxDataBatch is blocking, hence we can take these by
      // reference.
      [&](size_t index,
          folly::Try<std::shared_ptr<sapling::FileAuxData>> auxTry) {
        if (auxTry.hasException()) {
          XLOGF(
              DBG4,
              "Failed to import aux data node={} from EdenAPI (batch {}/{}): {}",
              folly::hexlify(requests[index].node),
              index,
              requests.size(),
              auxTry.exception().what().toStdString());
        } else {
          XLOGF(
              DBG4,
              "Imported aux data node={} from EdenAPI (batch: {}/{})",
              folly::hexlify(requests[index].node),
              index,
              requests.size());
        }

        if (auxTry.hasException()) {
          if (structuredLogger_ &&
              fetch_mode != sapling::FetchMode::RemoteOnly) {
            structuredLogger_->logEvent(FetchMiss{
                store_.getRepoName(),
                FetchMiss::BlobAuxData,
                auxTry.exception().what().toStdString(),
                false, // isRetry
                store_.dogfoodingHost()});
          }

          return;
        }

        const auto& nodeId = requests[index].node;
        XLOGF(DBG9, "Imported BlobAuxData={}", folly::hexlify(nodeId));
        auto& [importRequestList, watch] = importRequestsMap[nodeId];
        folly::Try<BlobAuxDataPtr> result;
        if (auxTry.hasException()) {
          result = folly::Try<BlobAuxDataPtr>{auxTry.exception()};
        } else {
          auto& aux = auxTry.value();
          result = folly::Try{std::make_shared<BlobAuxDataPtr::element_type>(
              Hash20{std::move(aux->content_sha1)},
              Hash32{std::move(aux->content_blake3)},
              aux->total_size)};
        }
        for (auto& importRequest : importRequestList) {
          importRequest->getPromise<BlobAuxDataPtr>()->setWith(
              [&]() -> folly::Try<BlobAuxDataPtr> { return result; });
        }

        // Make sure that we're stopping this watch.
        watch.reset();
      });
}

void SaplingBackingStore::processRequest() {
  folly::setThreadName("hgqueue");
  for (;;) {
    auto requests = queue_.dequeue();

    if (requests.empty()) {
      break;
    }

    const auto& first = requests.at(0);

    if (first->isType<SaplingImportRequest::BlobImport>()) {
      processBlobImportRequests(std::move(requests));
    } else if (first->isType<SaplingImportRequest::TreeImport>()) {
      processTreeImportRequests(std::move(requests));
    } else if (first->isType<SaplingImportRequest::BlobAuxImport>()) {
      processBlobAuxImportRequests(std::move(requests));
    } else if (first->isType<SaplingImportRequest::TreeAuxImport>()) {
      processTreeAuxImportRequests(std::move(requests));
    } else {
      XLOGF(DFATAL, "Unknown import request type: {}", first->getType());
    }
  }
}

ObjectComparison SaplingBackingStore::compareObjectsById(
    const ObjectId& one,
    const ObjectId& two) {
  // This is by far the common case, so check it first:
  if (one.bytesEqual(two)) {
    return ObjectComparison::Identical;
  }

  if (config_->getEdenConfig()->hgBijectiveBlobIds.getValue()) {
    // If one and two differ, and hg bijective blob IDs is enabled, then we know
    // the blob contents differ.
    return ObjectComparison::Different;
  }

  // Now parse the object IDs and read their rev hashes.
  auto oneProxy = HgProxyHash::load(
      localStore_.get(), one, "areObjectIdsEquivalent", *stats_);
  auto twoProxy = HgProxyHash::load(
      localStore_.get(), two, "areObjectIdsEquivalent", *stats_);

  // If the rev hashes are the same, we know the contents are the same.
  if (oneProxy.revHash() == twoProxy.revHash()) {
    return ObjectComparison::Identical;
  }

  // If rev hashes differ, and hg IDs aren't bijective, then we don't know
  // whether the IDs refer to the same contents or not.
  //
  // Mercurial's blob hashes also include history aux data, so there may be
  // multiple different blob hashes for the same file contents.
  return ObjectComparison::Unknown;
}

RootId SaplingBackingStore::parseRootId(folly::StringPiece rootId) {
  // rootId can be 20-byte binary or 40-byte hex. Canonicalize, unconditionally
  // returning 40-byte hex.
  return RootId{hash20FromThrift(rootId).toString()};
}

std::string SaplingBackingStore::renderRootId(const RootId& rootId) {
  // In memory, root IDs are stored as 40-byte hex. Thrift clients generally
  // expect 20-byte binary for Mercurial commit hashes, so re-encode that way.
  auto& value = rootId.value();
  if (value.size() == 40) {
    return folly::unhexlify(value);
  } else {
    XCHECK_EQ(0u, value.size());
    // Default-constructed RootId is the Mercurial null hash.
    return folly::unhexlify(kZeroHash.toString());
  }
}

ObjectId SaplingBackingStore::staticParseObjectId(folly::StringPiece objectId) {
  if (objectId.startsWith("proxy-")) {
    if (objectId.size() != 46) {
      throwf<std::invalid_argument>(
          "invalid proxy hash length: {}", objectId.size());
    }

    return ObjectId{folly::unhexlify<folly::fbstring>(objectId.subpiece(6))};
  }

  if (objectId.size() == 40) {
    return HgProxyHash::makeEmbeddedProxyHash2(Hash20{objectId});
  }

  if (objectId.size() < 41) {
    throwf<std::invalid_argument>("hg object ID too short: {}", objectId);
  }

  if (objectId[40] != ':') {
    throwf<std::invalid_argument>(
        "missing separator colon in hg object ID: {}", objectId);
  }

  Hash20 hgRevHash{objectId.subpiece(0, 40)};
  RelativePathPiece path{objectId.subpiece(41)};
  return HgProxyHash::makeEmbeddedProxyHash1(hgRevHash, path);
}

std::string SaplingBackingStore::staticRenderObjectId(
    const ObjectId& objectId) {
  if (auto proxyHash = HgProxyHash::tryParseEmbeddedProxyHash(objectId)) {
    if (proxyHash->path().empty()) {
      return folly::hexlify(proxyHash->byteHash());
    }
    return fmt::format(
        "{}:{}", folly::hexlify(proxyHash->byteHash()), proxyHash->path());
  }
  return fmt::format("proxy-{}", folly::hexlify(objectId.getBytes()));
}

folly::SemiFuture<BackingStore::GetTreeAuxResult>
SaplingBackingStore::getTreeAuxData(
    const ObjectId& id,
    const ObjectFetchContextPtr& context) {
  DurationScope<EdenStats> scope{
      stats_, &SaplingBackingStoreStats::getTreeAuxData};

  HgProxyHash proxyHash;
  try {
    proxyHash =
        HgProxyHash::load(localStore_.get(), id, "getTreeAuxData", *stats_);
  } catch (const std::exception&) {
    logMissingProxyHash();
    throw;
  }

  logBackingStoreFetch(
      *context,
      folly::Range{&proxyHash, 1},
      ObjectFetchContext::ObjectType::TreeAuxData);

  auto auxData = getLocalTreeAuxData(proxyHash);
  if (auxData.hasValue()) {
    stats_->increment(&SaplingBackingStoreStats::fetchTreeAuxDataSuccess);
    stats_->increment(&SaplingBackingStoreStats::fetchTreeAuxDataLocal);
    return folly::makeSemiFuture(GetTreeAuxResult{
        std::move(auxData.value()), ObjectFetchContext::Origin::FromDiskCache});
  }

  return getTreeAuxDataEnqueue(id, proxyHash, context)
      .ensure([scope = std::move(scope)] {})
      .semi();
}

ImmediateFuture<BackingStore::GetTreeAuxResult>
SaplingBackingStore::getTreeAuxDataEnqueue(
    const ObjectId& id,
    const HgProxyHash& proxyHash,
    const ObjectFetchContextPtr& context) {
  auto getTreeAuxFuture = makeImmediateFutureWith([&] {
    XLOGF(
        DBG4,
        "making tree aux data import request for {}",
        proxyHash.path(),
        id);
    auto requestContext = context.copy();
    auto request = SaplingImportRequest::makeTreeAuxImportRequest(
        id, proxyHash, requestContext);
    auto unique = request->getUnique();

    auto importTracker =
        std::make_unique<RequestMetricsScope>(&pendingImportTreeAuxWatches_);
    traceBus_->publish(HgImportTraceEvent::queue(
        unique,
        HgImportTraceEvent::TREE_AUX,
        proxyHash,
        context->getPriority().getClass(),
        context->getCause(),
        context->getClientPid()));

    return queue_.enqueueTreeAux(std::move(request))
        .ensure([this,
                 unique,
                 proxyHash,
                 context = context.copy(),
                 importTracker = std::move(importTracker)]() {
          traceBus_->publish(HgImportTraceEvent::finish(
              unique,
              HgImportTraceEvent::TREE_AUX,
              proxyHash,
              context->getPriority().getClass(),
              context->getCause(),
              context->getClientPid(),
              context->getFetchedSource()));
        });
  });

  return std::move(getTreeAuxFuture)
      .thenTry([this, id](folly::Try<TreeAuxDataPtr>&& result) {
        this->queue_.markImportAsFinished<TreeAuxDataPtr::element_type>(
            id, result);
        auto treeAux = std::move(result).value();
        return GetTreeAuxResult{
            std::move(treeAux), ObjectFetchContext::Origin::FromNetworkFetch};
      });
}

folly::Try<TreeAuxDataPtr> SaplingBackingStore::getLocalTreeAuxData(
    const HgProxyHash& hgInfo) {
  auto auxData = store_.getTreeAuxData(hgInfo.byteHash(), true /*local_only*/);

  using GetTreeAuxDataResult = folly::Try<TreeAuxDataPtr>;

  if (auxData.hasValue()) {
    return GetTreeAuxDataResult{
        std::make_shared<TreeAuxDataPtr::element_type>(TreeAuxData{
            Hash32{std::move(auxData.value()->digest_hash)},
            auxData.value()->digest_size})};
  } else {
    return GetTreeAuxDataResult{auxData.exception()};
  }
}

folly::SemiFuture<BackingStore::GetTreeResult> SaplingBackingStore::getTree(
    const ObjectId& id,
    const ObjectFetchContextPtr& context) {
  DurationScope<EdenStats> scope{stats_, &SaplingBackingStoreStats::getTree};

  HgProxyHash proxyHash;
  try {
    proxyHash = HgProxyHash::load(localStore_.get(), id, "getTree", *stats_);
  } catch (const std::exception&) {
    logMissingProxyHash();
    throw;
  }

  logBackingStoreFetch(
      *context,
      folly::Range{&proxyHash, 1},
      ObjectFetchContext::ObjectType::Tree);

  if (auto tree = getTreeLocal(id, proxyHash)) {
    XLOGF(
        DBG5,
        "imported tree of '{}', {} from hgcache",
        proxyHash.path(),
        proxyHash.revHash().toString());
    stats_->increment(&SaplingBackingStoreStats::fetchTreeSuccess);
    if (store_.dogfoodingHost()) {
      stats_->increment(&SaplingBackingStoreStats::fetchTreeSuccessDogfooding);
    }
    stats_->increment(&SaplingBackingStoreStats::fetchTreeLocal);
    return folly::makeSemiFuture(GetTreeResult{
        std::move(tree), ObjectFetchContext::Origin::FromDiskCache});
  }

  return getTreeEnqueue(id, proxyHash, context)
      .ensure([scope = std::move(scope)] {})
      .semi();
}

ImmediateFuture<BackingStore::GetTreeResult>
SaplingBackingStore::getTreeEnqueue(
    const ObjectId& id,
    const HgProxyHash& proxyHash,
    const ObjectFetchContextPtr& context) {
  auto getTreeFuture = makeImmediateFutureWith([&] {
    auto requestContext = context.copy();
    auto request = SaplingImportRequest::makeTreeImportRequest(
        id, proxyHash, requestContext);
    uint64_t unique = request->getUnique();

    auto importTracker =
        std::make_unique<RequestMetricsScope>(&pendingImportTreeWatches_);
    traceBus_->publish(HgImportTraceEvent::queue(
        unique,
        HgImportTraceEvent::TREE,
        proxyHash,
        context->getPriority().getClass(),
        context->getCause(),
        context->getClientPid()));

    return queue_.enqueueTree(std::move(request))
        .ensure([this,
                 unique,
                 proxyHash,
                 context = context.copy(),
                 importTracker = std::move(importTracker)]() {
          traceBus_->publish(HgImportTraceEvent::finish(
              unique,
              HgImportTraceEvent::TREE,
              proxyHash,
              context->getPriority().getClass(),
              context->getCause(),
              context->getClientPid(),
              context->getFetchedSource()));
        });
  });

  return std::move(getTreeFuture)
      .thenTry([this, id](folly::Try<TreePtr>&& result) {
        this->queue_.markImportAsFinished<TreePtr::element_type>(id, result);
        auto tree = std::move(result).value();
        return GetTreeResult{
            std::move(tree), ObjectFetchContext::Origin::FromNetworkFetch};
      });
}

TreePtr SaplingBackingStore::getTreeLocal(
    const ObjectId& edenTreeId,
    const HgProxyHash& proxyHash) {
  auto tree =
      store_.getTree(proxyHash.byteHash(), sapling::FetchMode::LocalOnly);
  if (tree.hasValue()) {
    auto hgObjectIdFormat =
        config_->getEdenConfig()->hgObjectIdFormat.getValue();
    return fromRawTree(
        tree.value().get(), edenTreeId, proxyHash.path(), hgObjectIdFormat);
  }

  return nullptr;
}

folly::Try<TreePtr> SaplingBackingStore::getTreeRemote(
    const RelativePath& path,
    const Hash20& manifestId,
    const ObjectId& edenTreeId,
    const ObjectFetchContextPtr& /*context*/) {
  auto tree = store_.getTree(
      manifestId.getBytes(),
      sapling::FetchMode::RemoteOnly /*, sapling::ClientRequestInfo(context)*/);

  using GetTreeResult = folly::Try<TreePtr>;

  if (tree.hasValue()) {
    auto hgObjectIdFormat =
        config_->getEdenConfig()->hgObjectIdFormat.getValue();
    return GetTreeResult{fromRawTree(
        tree.value().get(), edenTreeId, path, std::move(hgObjectIdFormat))};
  } else {
    return GetTreeResult{tree.exception()};
  }
}

folly::SemiFuture<BackingStore::GetBlobResult> SaplingBackingStore::getBlob(
    const ObjectId& id,
    const ObjectFetchContextPtr& context) {
  DurationScope<EdenStats> scope{stats_, &SaplingBackingStoreStats::getBlob};

  HgProxyHash proxyHash;
  try {
    proxyHash = HgProxyHash::load(localStore_.get(), id, "getBlob", *stats_);
  } catch (const std::exception&) {
    logMissingProxyHash();
    throw;
  }

  logBackingStoreFetch(
      *context,
      folly::Range{&proxyHash, 1},
      ObjectFetchContext::ObjectType::Blob);

  auto blob = getBlobLocal(proxyHash);
  if (blob.hasValue()) {
    stats_->increment(&SaplingBackingStoreStats::fetchBlobSuccess);
    if (store_.dogfoodingHost()) {
      stats_->increment(&SaplingBackingStoreStats::fetchBlobSuccessDogfooding);
    }
    stats_->increment(&SaplingBackingStoreStats::fetchBlobLocal);
    return folly::makeSemiFuture(GetBlobResult{
        std::move(blob.value()), ObjectFetchContext::Origin::FromDiskCache});
  }

  return getBlobEnqueue(
             id, proxyHash, context, SaplingImportRequest::FetchType::Fetch)
      .ensure([scope = std::move(scope)] {})
      .semi();
}

ImmediateFuture<BackingStore::GetBlobResult>
SaplingBackingStore::getBlobEnqueue(
    const ObjectId& id,
    const HgProxyHash& proxyHash,
    const ObjectFetchContextPtr& context,
    const SaplingImportRequest::FetchType fetch_type) {
  auto getBlobFuture = makeImmediateFutureWith([&] {
    XLOGF(
        DBG4,
        "making blob import request for {}, hash is: {}",
        proxyHash.path(),
        id);
    auto requestContext = context.copy();
    auto request = SaplingImportRequest::makeBlobImportRequest(
        id, proxyHash, requestContext);
    request->setFetchType(fetch_type);
    auto unique = request->getUnique();
    std::unique_ptr<RequestMetricsScope> importTracker;
    switch (fetch_type) {
      case SaplingImportRequest::FetchType::Fetch:
        importTracker =
            std::make_unique<RequestMetricsScope>(&pendingImportBlobWatches_);
        break;
      case SaplingImportRequest::FetchType::Prefetch:
        importTracker = std::make_unique<RequestMetricsScope>(
            &pendingImportPrefetchWatches_);
        break;
    }
    traceBus_->publish(HgImportTraceEvent::queue(
        unique,
        HgImportTraceEvent::BLOB,
        proxyHash,
        context->getPriority().getClass(),
        context->getCause(),
        context->getClientPid()));

    return queue_.enqueueBlob(std::move(request))
        .ensure([this,
                 unique,
                 proxyHash,
                 context = context.copy(),
                 importTracker = std::move(importTracker)]() {
          traceBus_->publish(HgImportTraceEvent::finish(
              unique,
              HgImportTraceEvent::BLOB,
              proxyHash,
              context->getPriority().getClass(),
              context->getCause(),
              context->getClientPid(),
              context->getFetchedSource()));
        });
  });

  return std::move(getBlobFuture)
      .thenTry([this, id](folly::Try<BlobPtr>&& result) {
        this->queue_.markImportAsFinished<BlobPtr::element_type>(id, result);
        auto blob = std::move(result).value();
        return GetBlobResult{
            std::move(blob), ObjectFetchContext::Origin::FromNetworkFetch};
      });
}

folly::SemiFuture<BackingStore::GetBlobAuxResult>
SaplingBackingStore::getBlobAuxData(
    const ObjectId& id,
    const ObjectFetchContextPtr& context) {
  DurationScope<EdenStats> scope{
      stats_, &SaplingBackingStoreStats::getBlobAuxData};

  HgProxyHash proxyHash;
  try {
    proxyHash =
        HgProxyHash::load(localStore_.get(), id, "getBlobAuxData", *stats_);
  } catch (const std::exception&) {
    logMissingProxyHash();
    throw;
  }

  logBackingStoreFetch(
      *context,
      folly::Range{&proxyHash, 1},
      ObjectFetchContext::ObjectType::BlobAuxData);

  auto auxData = getLocalBlobAuxData(proxyHash);
  if (auxData.hasValue()) {
    stats_->increment(&SaplingBackingStoreStats::fetchBlobAuxDataSuccess);
    stats_->increment(&SaplingBackingStoreStats::fetchBlobAuxDataLocal);
    return folly::makeSemiFuture(GetBlobAuxResult{
        std::move(auxData.value()), ObjectFetchContext::Origin::FromDiskCache});
  }

  return getBlobAuxDataEnqueue(id, proxyHash, context)
      .ensure([scope = std::move(scope)] {})
      .semi();
}

ImmediateFuture<BackingStore::GetBlobAuxResult>
SaplingBackingStore::getBlobAuxDataEnqueue(
    const ObjectId& id,
    const HgProxyHash& proxyHash,
    const ObjectFetchContextPtr& context) {
  if (!config_->getEdenConfig()->fetchHgAuxMetadata.getValue()) {
    return BackingStore::GetBlobAuxResult{
        nullptr, ObjectFetchContext::Origin::NotFetched};
  }

  auto getBlobAuxFuture = makeImmediateFutureWith([&] {
    XLOGF(
        DBG4,
        "making blob meta import request for {}, hash is: {}",
        proxyHash.path(),
        id);
    auto requestContext = context.copy();
    auto request = SaplingImportRequest::makeBlobAuxImportRequest(
        id, proxyHash, requestContext);
    auto unique = request->getUnique();

    auto importTracker =
        std::make_unique<RequestMetricsScope>(&pendingImportBlobAuxWatches_);
    traceBus_->publish(HgImportTraceEvent::queue(
        unique,
        HgImportTraceEvent::BLOB_AUX,
        proxyHash,
        context->getPriority().getClass(),
        context->getCause(),
        context->getClientPid()));

    return queue_.enqueueBlobAux(std::move(request))
        .ensure([this,
                 unique,
                 proxyHash,
                 context = context.copy(),
                 importTracker = std::move(importTracker)]() {
          traceBus_->publish(HgImportTraceEvent::finish(
              unique,
              HgImportTraceEvent::BLOB_AUX,
              proxyHash,
              context->getPriority().getClass(),
              context->getCause(),
              context->getClientPid(),
              context->getFetchedSource()));
        });
  });

  return std::move(getBlobAuxFuture)
      .thenTry([this, id](folly::Try<BlobAuxDataPtr>&& result) {
        this->queue_.markImportAsFinished<BlobAuxDataPtr::element_type>(
            id, result);
        auto blobAux = std::move(result).value();
        return GetBlobAuxResult{
            std::move(blobAux), ObjectFetchContext::Origin::FromNetworkFetch};
      });
}

folly::Try<BlobAuxDataPtr> SaplingBackingStore::getLocalBlobAuxData(
    const HgProxyHash& hgInfo) {
  auto auxData = store_.getBlobAuxData(hgInfo.byteHash(), true /*local_only*/);

  using GetBlobAuxDataResult = folly::Try<BlobAuxDataPtr>;

  if (auxData.hasValue()) {
    return GetBlobAuxDataResult{
        std::make_shared<BlobAuxDataPtr::element_type>(BlobAuxData{
            Hash20{std::move(auxData.value()->content_sha1)},
            Hash32{std::move(auxData.value()->content_blake3)},
            auxData.value()->total_size})};
  } else {
    return GetBlobAuxDataResult{auxData.exception()};
  }
}

ImmediateFuture<BackingStore::GetRootTreeResult>
SaplingBackingStore::getRootTree(
    const RootId& rootId,
    const ObjectFetchContextPtr& context) {
  folly::stop_watch<std::chrono::milliseconds> watch;
  ObjectId commitId = hashFromRootId(rootId);

  return localStore_
      ->getImmediateFuture(KeySpace::HgCommitToTreeFamily, commitId)
      .thenValue(
          [this, commitId, context = context.copy(), watch](StoreResult result)
              -> folly::SemiFuture<BackingStore::GetRootTreeResult> {
            if (!result.isValid()) {
              return importTreeManifest(
                         commitId,
                         context,
                         ObjectFetchContext::ObjectType::RootTree)
                  .thenValue([this, commitId, watch](TreePtr rootTree) {
                    XLOGF(
                        DBG1,
                        "imported mercurial commit {} as tree {}",
                        commitId,
                        rootTree->getHash());
                    stats_->addDuration(
                        &SaplingBackingStoreStats::getRootTree,
                        watch.elapsed());
                    localStore_->put(
                        KeySpace::HgCommitToTreeFamily,
                        commitId,
                        rootTree->getHash().getBytes());
                    return BackingStore::GetRootTreeResult{
                        rootTree, rootTree->getHash()};
                  });
            }

            auto rootTreeHash = HgProxyHash::load(
                localStore_.get(),
                ObjectId{result.bytes()},
                "getRootTree",
                *stats_);
            return importTreeManifestImpl(
                       rootTreeHash.revHash(),
                       context,
                       ObjectFetchContext::ObjectType::RootTree)
                .thenValue([this, watch](TreePtr tree) {
                  stats_->addDuration(
                      &SaplingBackingStoreStats::getRootTree, watch.elapsed());
                  return BackingStore::GetRootTreeResult{tree, tree->getHash()};
                });
          });
}

folly::Future<TreePtr> SaplingBackingStore::importTreeManifest(
    const ObjectId& commitId,
    const ObjectFetchContextPtr& context,
    const ObjectFetchContext::ObjectType type) {
  return folly::via(
             serverThreadPool_,
             [this, commitId] { return getManifestNode(commitId); })
      .thenValue([this, commitId, fetchContext = context.copy(), type](
                     auto manifestNode) {
        if (!manifestNode.has_value()) {
          auto ew = folly::exception_wrapper{std::runtime_error{
              "Manifest node could not be found for commitId"}};
          return folly::makeFuture<TreePtr>(std::move(ew));
        }
        XLOGF(
            DBG2,
            "commit {} has manifest node {}",
            commitId,
            manifestNode.value());
        return importTreeManifestImpl(
            *std::move(manifestNode), fetchContext, type);
      });
}

std::optional<Hash20> SaplingBackingStore::getManifestNode(
    const ObjectId& commitId) {
  auto manifestNode = store_.getManifestNode(commitId.getBytes());
  if (!manifestNode.has_value()) {
    XLOGF(DBG2, "Error while getting manifest node from datapackstore");
    return std::nullopt;
  }
  return Hash20(*std::move(manifestNode));
}

folly::Future<TreePtr> SaplingBackingStore::importTreeManifestImpl(
    Hash20 manifestNode,
    const ObjectFetchContextPtr& context,
    const ObjectFetchContext::ObjectType type) {
  // Record that we are at the root for this node
  RelativePathPiece path{};
  auto hgObjectIdFormat = config_->getEdenConfig()->hgObjectIdFormat.getValue();

  ObjectId objectId;

  switch (hgObjectIdFormat) {
    case HgObjectIdFormat::WithPath:
      objectId = HgProxyHash::makeEmbeddedProxyHash1(manifestNode, path);
      break;

    case HgObjectIdFormat::HashOnly:
      objectId = HgProxyHash::makeEmbeddedProxyHash2(manifestNode);
      break;
  }

  // try SaplingNativeBackingStore
  auto tree = getTreeFromBackingStore(
      path.copy(), manifestNode, objectId, context.copy(), type);
  if (tree.hasValue()) {
    XLOGF(
        DBG4,
        "imported tree node={} path={} from SaplingNativeBackingStore",
        manifestNode,
        path);
    switch (type) {
      case ObjectFetchContext::ObjectType::Tree:
        // getTree never gets here. We add this case only for completeness
        stats_->increment(&SaplingBackingStoreStats::fetchTreeSuccess);
        break;
      case ObjectFetchContext::ObjectType::RootTree:
        stats_->increment(&SaplingBackingStoreStats::getRootTreeSuccess);
        break;
      case ObjectFetchContext::ObjectType::ManifestForRoot:
        stats_->increment(
            &SaplingBackingStoreStats::importManifestForRootSuccess);
        break;
      // The following types cannot get here. It is just for completeness
      case ObjectFetchContext::TreeAuxData:
      case ObjectFetchContext::Blob:
      case ObjectFetchContext::BlobAuxData:
      case ObjectFetchContext::ObjectType::PrefetchBlob:
      case ObjectFetchContext::kObjectTypeEnumMax:
        break;
    }
    if (store_.dogfoodingHost()) {
      stats_->increment(&SaplingBackingStoreStats::fetchTreeSuccessDogfooding);
    }
    return folly::makeFuture(std::move(tree.value()));
  }
  // retry once if the initial fetch failed
  switch (type) {
    case ObjectFetchContext::ObjectType::Tree:
      // getTree never gets here. We add this case only for completeness
      stats_->increment(&SaplingBackingStoreStats::fetchTreeFailure);
      break;
    case ObjectFetchContext::ObjectType::RootTree:
      stats_->increment(&SaplingBackingStoreStats::getRootTreeFailure);
      break;
    case ObjectFetchContext::ObjectType::ManifestForRoot:
      stats_->increment(
          &SaplingBackingStoreStats::importManifestForRootFailure);
      break;
      // The following types cannot get here. It is just for completeness
    case ObjectFetchContext::TreeAuxData:
    case ObjectFetchContext::Blob:
    case ObjectFetchContext::BlobAuxData:
    case ObjectFetchContext::PrefetchBlob:
    case ObjectFetchContext::kObjectTypeEnumMax:
      break;
  }
  if (store_.dogfoodingHost()) {
    stats_->increment(&SaplingBackingStoreStats::fetchTreeFailureDogfooding);
  }
  return retryGetTree(manifestNode, objectId, path, context.copy(), type);
}

folly::Future<TreePtr> SaplingBackingStore::retryGetTree(
    const Hash20& manifestNode,
    const ObjectId& edenTreeID,
    RelativePathPiece path,
    ObjectFetchContextPtr context,
    const ObjectFetchContext::ObjectFetchContext::ObjectType type) {
  XLOGF(
      DBG6,
      "importing tree {}: hg manifest {} for path \"{}\"",
      edenTreeID,
      manifestNode,
      path);

  // Explicitly check for the null ID on the root directory.
  // This isn't actually present in the mercurial data store; it has to be
  // handled specially in the code.
  if (path.empty() && manifestNode == kZeroHash) {
    auto tree = std::make_shared<TreePtr::element_type>(
        Tree::container{kPathMapDefaultCaseSensitive}, edenTreeID);
    return folly::makeFuture(std::move(tree));
  }

  if (!FLAGS_hg_fetch_missing_trees) {
    auto ew = folly::exception_wrapper{std::runtime_error{
        "Data not available via edenapi, skipping fallback to importer because "
        "of FLAGS_hg_fetch_missing_trees"}};
    return folly::makeFuture<TreePtr>(std::move(ew));
  }

  auto writeBatch = localStore_->beginWrite();
  // When aux aux data is enabled hg fetches file aux data along with get tree
  // request, no need for separate network call!
  return retryGetTreeImpl(
             manifestNode,
             edenTreeID,
             path.copy(),
             std::move(writeBatch),
             context.copy(),
             type)
      .thenValue([config = config_](TreePtr&& result) mutable {
        return std::move(result);
      });
}

folly::Try<TreePtr> SaplingBackingStore::getTreeFromBackingStore(
    const RelativePath& path,
    const Hash20& manifestId,
    const ObjectId& edenTreeId,
    ObjectFetchContextPtr context,
    const ObjectFetchContext::ObjectType type) {
  folly::Try<std::shared_ptr<sapling::Tree>> tree;
  sapling::FetchMode fetch_mode = sapling::FetchMode::AllowRemote;
  if (config_->getEdenConfig()->allowRemoteGetBatch.getValue()) {
    // For root trees we will try getting the tree locally first.  This allows
    // us to catch when Mercurial might have just written a tree to the store,
    // and refresh the store so that the store can pick it up.  We don't do
    // this for all trees, as it would cause a lot of additional work on every
    // cache miss, and just doing it for root trees is sufficient to detect the
    // scenario where Mercurial just wrote a brand new tree.
    if (path.empty()) {
      fetch_mode = sapling::FetchMode::LocalOnly;
    }
    tree = store_.getTree(manifestId.getBytes(), fetch_mode);
    if (tree.hasException() && fetch_mode == sapling::FetchMode::LocalOnly) {
      // Mercurial might have just written the tree to the store. Refresh the
      // store and try again, this time allowing remote fetches.
      store_.flush();
      fetch_mode = sapling::FetchMode::AllowRemote;
      tree = store_.getTree(manifestId.getBytes(), fetch_mode);
    }
  } else {
    fetch_mode = sapling::FetchMode::LocalOnly;
    tree = store_.getTree(manifestId.getBytes(), fetch_mode);
    if (tree.hasException()) {
      if (path.empty()) {
        // This allows us to catch when Mercurial might have just written a tree
        // to the store, and refresh the store so that the store can pick it up.
        // We don't do this for all trees, as it would cause a lot of additional
        // work on every cache miss, and just doing it for root trees is
        // sufficient to detect the scenario where Mercurial just wrote a brand
        // new tree.
        store_.flush();
      }
      fetch_mode = sapling::FetchMode::RemoteOnly;
      tree = store_.getTree(manifestId.getBytes(), fetch_mode);
    }
  }

  using GetTreeResult = folly::Try<TreePtr>;

  if (tree.hasValue()) {
    auto hgObjectIdFormat =
        config_->getEdenConfig()->hgObjectIdFormat.getValue();
    switch (fetch_mode) {
      case sapling::FetchMode::LocalOnly:
        context->setFetchedSource(
            ObjectFetchContext::FetchedSource::Local, type, stats_.copy());
        break;
      case sapling::FetchMode::RemoteOnly:
        context->setFetchedSource(
            ObjectFetchContext::FetchedSource::Remote, type, stats_.copy());
        break;
      case sapling::FetchMode::AllowRemote:
      case sapling::FetchMode::AllowRemotePrefetch:
        context->setFetchedSource(
            ObjectFetchContext::FetchedSource::Unknown, type, stats_.copy());
        break;
    }
    return GetTreeResult{fromRawTree(
        tree.value().get(), edenTreeId, path, std::move(hgObjectIdFormat))};
  } else {
    return GetTreeResult{tree.exception()};
  }
}

folly::Future<TreePtr> SaplingBackingStore::retryGetTreeImpl(
    Hash20 manifestNode,
    ObjectId edenTreeID,
    RelativePath path,
    std::shared_ptr<LocalStore::WriteBatch> writeBatch,
    ObjectFetchContextPtr context,
    const ObjectFetchContext::ObjectType type) {
  return folly::via(
      retryThreadPool_.get(),
      [this,
       path = std::move(path),
       manifestNode,
       edenTreeID = std::move(edenTreeID),
       writeBatch,
       &liveImportTreeWatches = liveImportTreeWatches_,
       context = context.copy(),
       type] {
        RequestMetricsScope queueTracker{&liveImportTreeWatches};

        // NOTE: In the future we plan to update
        // SaplingNativeBackingStore to provide and
        // asynchronous interface enabling us to perform our retries
        // there. In the meantime we use retryThreadPool_ for these
        // longer-running retry requests to avoid starving
        // serverThreadPool_.

        // Flush (and refresh) SaplingNativeBackingStore to ensure all
        // data is written and to rescan pack files or local indexes
        flush();

        // Retry using SaplingNativeBackingStore
        auto result = folly::makeFuture<TreePtr>(TreePtr{nullptr});
        auto tree = getTreeFromBackingStore(
            path, manifestNode, edenTreeID, context.copy(), type);
        if (tree.hasValue()) {
          switch (type) {
            case ObjectFetchContext::ObjectType::Tree:
              stats_->increment(
                  &SaplingBackingStoreStats::fetchTreeRetrySuccess);
              break;
            case ObjectFetchContext::ObjectType::RootTree:
              stats_->increment(
                  &SaplingBackingStoreStats::getRootTreeRetrySuccess);
              break;
            case ObjectFetchContext::ObjectType::ManifestForRoot:
              stats_->increment(
                  &SaplingBackingStoreStats::importManifestForRootRetrySuccess);
              break;
            // The following types cannot get here. It is just for completeness
            case ObjectFetchContext::TreeAuxData:
            case ObjectFetchContext::Blob:
            case ObjectFetchContext::BlobAuxData:
            case ObjectFetchContext::PrefetchBlob:
            case ObjectFetchContext::kObjectTypeEnumMax:
              break;
          }
          if (store_.dogfoodingHost()) {
            stats_->increment(
                &SaplingBackingStoreStats::fetchTreeRetrySuccessDogfooding);
          }
          result = tree.value();
        } else {
          // Record miss and return error
          if (structuredLogger_) {
            structuredLogger_->logEvent(FetchMiss{
                store_.getRepoName(),
                FetchMiss::Tree,
                tree.exception().what().toStdString(),
                true, // isRetry
                store_.dogfoodingHost()});
          }

          switch (type) {
            case ObjectFetchContext::ObjectType::Tree:
              stats_->increment(
                  &SaplingBackingStoreStats::fetchTreeRetryFailure);
              break;
            case ObjectFetchContext::ObjectType::RootTree:
              stats_->increment(
                  &SaplingBackingStoreStats::getRootTreeRetryFailure);
              break;
            case ObjectFetchContext::ObjectType::ManifestForRoot:
              stats_->increment(
                  &SaplingBackingStoreStats::importManifestForRootRetryFailure);
              break;
            // The following types cannot get here. It is just for completeness
            case ObjectFetchContext::TreeAuxData:
            case ObjectFetchContext::Blob:
            case ObjectFetchContext::BlobAuxData:
            case ObjectFetchContext::PrefetchBlob:
            case ObjectFetchContext::kObjectTypeEnumMax:
              break;
          }
          if (store_.dogfoodingHost()) {
            stats_->increment(
                &SaplingBackingStoreStats::fetchTreeRetryFailureDogfooding);
          }
          auto ew = folly::exception_wrapper{tree.exception()};
          result = folly::makeFuture<TreePtr>(std::move(ew));
        }
        return result;
      });
}

folly::SemiFuture<folly::Unit> SaplingBackingStore::prefetchBlobs(
    ObjectIdRange ids,
    const ObjectFetchContextPtr& context) {
  return HgProxyHash::getBatch(localStore_.get(), ids, *stats_)
      // The caller guarantees that ids will live at least longer than this
      // future, thus we don't need to deep-copy it.
      .thenTry([context = context.copy(), this, ids](
                   folly::Try<std::vector<HgProxyHash>> tryHashes) {
        if (tryHashes.hasException()) {
          logMissingProxyHash();
        }
        auto& proxyHashes = tryHashes.value();

        logBackingStoreFetch(
            *context,
            folly::Range{proxyHashes.data(), proxyHashes.size()},
            ObjectFetchContext::ObjectType::Blob);

        // Do not check for whether blobs are already present locally, this
        // check is useful for latency oriented workflows, not for throughput
        // oriented ones. Mercurial will anyway not re-fetch a blob that is
        // already present locally, so the check for local blob is pure overhead
        // when prefetching.
        std::vector<ImmediateFuture<GetBlobResult>> futures;
        futures.reserve(ids.size());

        for (size_t i = 0; i < ids.size(); i++) {
          const auto& id = ids[i];
          const auto& proxyHash = proxyHashes[i];

          futures.emplace_back(getBlobEnqueue(
              id,
              proxyHash,
              context,
              SaplingImportRequest::FetchType::Prefetch));
        }

        return collectAllSafe(std::move(futures)).unit();
      })
      .semi();
}

ImmediateFuture<BackingStore::GetGlobFilesResult>
SaplingBackingStore::getGlobFiles(
    const RootId& id,
    const std::vector<std::string>& globs) {
  folly::stop_watch<std::chrono::milliseconds> watch;
  using GetGlobFilesResult = folly::Try<GetGlobFilesResult>;
  auto globFilesResult = store_.getGlobFiles(id.value(), globs);

  if (globFilesResult.hasValue()) {
    std::vector<std::string> files;
    auto globFiles = globFilesResult.value()->files;
    for (auto& file : globFiles) {
      files.emplace_back(file);
    }
    stats_->addDuration(
        &SaplingBackingStoreStats::fetchGlobFiles, watch.elapsed());
    stats_->increment(&SaplingBackingStoreStats::fetchGlobFilesSuccess);

    return GetGlobFilesResult{BackingStore::GetGlobFilesResult{files, id}};
  } else {
    stats_->increment(&SaplingBackingStoreStats::fetchGlobFilesFailure);
    return GetGlobFilesResult{globFilesResult.exception()};
  }
}

void SaplingBackingStore::logMissingProxyHash() {
  auto now = std::chrono::steady_clock::now();

  bool shouldLog = false;
  {
    auto last = lastMissingProxyHashLog_.wlock();
    if (now >= *last +
            config_->getEdenConfig()
                ->missingHgProxyHashLogInterval.getValue()) {
      shouldLog = true;
      *last = now;
    }
  }

  if (shouldLog) {
    structuredLogger_->logEvent(MissingProxyHash{});
  }
}

void SaplingBackingStore::logBackingStoreFetch(
    const ObjectFetchContext& context,
    folly::Range<HgProxyHash*> hashes,
    ObjectFetchContext::ObjectType type) {
  const auto& logFetchPathRegex =
      config_->getEdenConfig()->logObjectFetchPathRegex.getValue();

  if (logFetchPathRegex) {
    for (const auto& hash : hashes) {
      auto path = hash.path();
      auto pathPiece = path.view();

      if (RE2::PartialMatch(
              re2::StringPiece{pathPiece.data(), pathPiece.size()},
              **logFetchPathRegex)) {
        logger_->logImport(context, path, type);
      }
    }
  }

  if (type != ObjectFetchContext::ObjectType::Tree &&
      isRecordingFetch_.load(std::memory_order_relaxed) &&
      context.getCause() != ObjectFetchContext::Cause::Prefetch) {
    auto guard = fetchedFilePaths_.wlock();
    for (const auto& hash : hashes) {
      guard->emplace(hash.path().view());
    }
  }
}

size_t SaplingBackingStore::getImportMetric(
    RequestMetricsScope::RequestStage stage,
    SaplingImportObject object,
    RequestMetricsScope::RequestMetric metric) const {
  return RequestMetricsScope::getMetricFromWatches(
      metric, getImportWatches(stage, object));
}

RequestMetricsScope::LockedRequestWatchList&
SaplingBackingStore::getImportWatches(
    RequestMetricsScope::RequestStage stage,
    SaplingImportObject object) const {
  switch (stage) {
    case RequestMetricsScope::RequestStage::PENDING:
      return getPendingImportWatches(object);
    case RequestMetricsScope::RequestStage::LIVE:
      return getLiveImportWatches(object);
  }
  EDEN_BUG() << "unknown sapling import stage " << enumValue(stage);
}

RequestMetricsScope::LockedRequestWatchList&
SaplingBackingStore::getPendingImportWatches(SaplingImportObject object) const {
  switch (object) {
    case SaplingImportObject::BLOB:
    case SaplingImportObject::BATCHED_BLOB:
      return pendingImportBlobWatches_;
    case SaplingImportObject::TREE:
    case SaplingImportObject::BATCHED_TREE:
      return pendingImportTreeWatches_;
    case SaplingImportObject::BLOB_AUX:
    case SaplingImportObject::BATCHED_BLOB_AUX:
      return pendingImportBlobAuxWatches_;
    case SaplingImportObject::TREE_AUX:
    case SaplingImportObject::BATCHED_TREE_AUX:
      return pendingImportTreeAuxWatches_;
    case SaplingImportObject::PREFETCH:
      return pendingImportPrefetchWatches_;
  }
  EDEN_BUG() << "unknown sapling import object type "
             << static_cast<int>(object);
}

RequestMetricsScope::LockedRequestWatchList&
SaplingBackingStore::getLiveImportWatches(SaplingImportObject object) const {
  switch (object) {
    case SaplingImportObject::BLOB:
      return liveImportBlobWatches_;
    case SaplingImportObject::TREE:
      return liveImportTreeWatches_;
    case SaplingImportObject::BLOB_AUX:
      return liveImportBlobAuxWatches_;
    case SaplingImportObject::TREE_AUX:
      return liveImportTreeAuxWatches_;
    case SaplingImportObject::PREFETCH:
      return liveImportPrefetchWatches_;
    case SaplingImportObject::BATCHED_BLOB:
      return liveBatchedBlobWatches_;
    case SaplingImportObject::BATCHED_TREE:
      return liveBatchedTreeWatches_;
    case SaplingImportObject::BATCHED_BLOB_AUX:
      return liveBatchedBlobAuxWatches_;
    case SaplingImportObject::BATCHED_TREE_AUX:
      return liveBatchedTreeAuxWatches_;
  }
  EDEN_BUG() << "unknown sapling import object " << enumValue(object);
}

folly::StringPiece SaplingBackingStore::stringOfSaplingImportObject(
    SaplingImportObject object) {
  switch (object) {
    case SaplingImportObject::BLOB:
      return "blob";
    case SaplingImportObject::TREE:
      return "tree";
    case SaplingImportObject::BLOB_AUX:
      return "blobmeta";
    case SaplingImportObject::TREE_AUX:
      return "treemeta";
    case SaplingImportObject::BATCHED_BLOB:
      return "batched_blob";
    case SaplingImportObject::BATCHED_TREE:
      return "batched_tree";
    case SaplingImportObject::BATCHED_BLOB_AUX:
      return "batched_blobmeta";
    case SaplingImportObject::BATCHED_TREE_AUX:
      return "batched_treemeta";
    case SaplingImportObject::PREFETCH:
      return "prefetch";
  }
  EDEN_BUG() << "unknown sapling import object " << enumValue(object);
}

void SaplingBackingStore::startRecordingFetch() {
  isRecordingFetch_.store(true, std::memory_order_relaxed);
}

std::unordered_set<std::string> SaplingBackingStore::stopRecordingFetch() {
  isRecordingFetch_.store(false, std::memory_order_relaxed);
  std::unordered_set<std::string> paths;
  std::swap(paths, *fetchedFilePaths_.wlock());
  return paths;
}

ImmediateFuture<folly::Unit> SaplingBackingStore::importManifestForRoot(
    const RootId& rootId,
    const Hash20& manifestId,
    const ObjectFetchContextPtr& context) {
  // This method is used when the client informs us about a target manifest
  // that it is about to update to, for the scenario when a manifest has
  // just been created.  Since the manifest has just been created locally, and
  // aux data is only available remotely, there will be no aux data available
  // to prefetch.
  //
  // When the local store is populated with aux data for newly-created
  // manifests then we can update this so that is true when appropriate.
  /**
   * Import the root manifest for the specied revision using mercurial
   * treemanifest data.  This is called when the root manifest is provided
   * to EdenFS directly by the hg client.
   */
  folly::stop_watch<std::chrono::milliseconds> watch;
  auto commitId = hashFromRootId(rootId);
  return localStore_
      ->getImmediateFuture(KeySpace::HgCommitToTreeFamily, commitId)
      .thenValue(
          [this, commitId, manifestId, context = context.copy(), watch](
              StoreResult result) -> folly::Future<folly::Unit> {
            if (result.isValid()) {
              // We have already imported this commit, nothing to do.
              return folly::unit;
            }

            return importTreeManifestImpl(
                       manifestId,
                       context,
                       ObjectFetchContext::ObjectType::ManifestForRoot)
                .thenValue([this, commitId, manifestId, watch](
                               TreePtr rootTree) {
                  XLOGF(
                      DBG3,
                      "imported mercurial commit {} with manifest {} as tree {}",
                      commitId,
                      manifestId,
                      rootTree->getHash());
                  stats_->addDuration(
                      &SaplingBackingStoreStats::importManifestForRoot,
                      watch.elapsed());
                  localStore_->put(
                      KeySpace::HgCommitToTreeFamily,
                      commitId,
                      rootTree->getHash().getBytes());
                });
          });
}

void SaplingBackingStore::periodicManagementTask() {
  flush();
}

namespace {
void dropBlobImportRequest(std::shared_ptr<SaplingImportRequest>& request) {
  auto* promise = request->getPromise<BlobPtr>();
  if (promise != nullptr) {
    if (!promise->isFulfilled()) {
      promise->setException(std::runtime_error("Request forcibly dropped"));
    }
  }
}

void dropTreeImportRequest(std::shared_ptr<SaplingImportRequest>& request) {
  auto* promise = request->getPromise<TreePtr>();
  if (promise != nullptr) {
    if (!promise->isFulfilled()) {
      promise->setException(std::runtime_error("Request forcibly dropped"));
    }
  }
}
} // namespace

int64_t SaplingBackingStore::dropAllPendingRequestsFromQueue() {
  auto requestVec = queue_.combineAndClearRequestQueues();
  for (auto& request : requestVec) {
    if (request->isType<SaplingImportRequest::BlobImport>()) {
      XLOG(DBG7, "Dropping blob request");
      dropBlobImportRequest(request);
    } else if (request->isType<SaplingImportRequest::TreeImport>()) {
      XLOG(DBG7, "Dropping tree request");
      dropTreeImportRequest(request);
    }
  }
  return requestVec.size();
}

} // namespace facebook::eden
