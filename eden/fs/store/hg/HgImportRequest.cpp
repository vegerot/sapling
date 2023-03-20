/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#include "eden/fs/store/hg/HgImportRequest.h"

#include <folly/Try.h>
#include <folly/futures/Future.h>
#include <folly/futures/Promise.h>

#include "eden/fs/telemetry/RequestMetricsScope.h"

namespace facebook::eden {

template <typename RequestType>
HgImportRequest::HgImportRequest(
    RequestType request,
    ImportPriority priority,
    ObjectFetchContext::Cause cause,
    folly::Promise<typename RequestType::Response>&& promise)
    : request_(std::move(request)),
      priority_(priority),
      cause_(cause),
      promise_(std::move(promise)) {}

template <typename RequestType, typename... Input>
std::shared_ptr<HgImportRequest> HgImportRequest::makeRequest(
    ImportPriority priority,
    ObjectFetchContext::Cause cause,
    Input&&... input) {
  auto promise = folly::Promise<typename RequestType::Response>{};
  return std::make_shared<HgImportRequest>(
      RequestType{std::forward<Input>(input)...},
      priority,
      cause,
      std::move(promise));
}

std::shared_ptr<HgImportRequest> HgImportRequest::makeBlobImportRequest(
    const ObjectId& hash,
    const HgProxyHash& proxyHash,
    ImportPriority priority,
    ObjectFetchContext::Cause cause) {
  return makeRequest<BlobImport>(priority, cause, hash, proxyHash);
}

std::shared_ptr<HgImportRequest> HgImportRequest::makeTreeImportRequest(
    const ObjectId& hash,
    const HgProxyHash& proxyHash,
    ImportPriority priority,
    ObjectFetchContext::Cause cause) {
  return makeRequest<TreeImport>(priority, cause, hash, proxyHash);
}

std::shared_ptr<HgImportRequest> HgImportRequest::makeBlobMetaImportRequest(
    const ObjectId& hash,
    const HgProxyHash& proxyHash,
    ImportPriority priority,
    ObjectFetchContext::Cause cause) {
  return makeRequest<BlobMetaImport>(priority, cause, hash, proxyHash);
}

} // namespace facebook::eden
