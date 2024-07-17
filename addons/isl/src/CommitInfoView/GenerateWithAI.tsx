/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {Result} from '../types';
import type {RefObject} from 'react';
import type {Comparison} from 'shared/Comparison';

import {Internal} from '../Internal';
import {tracker} from '../analytics';
import {useFeatureFlagSync} from '../featureFlags';
import {T, t} from '../i18n';
import {atomFamilyWeak, atomLoadableWithRefresh, readAtom} from '../jotaiUtils';
import {uncommittedChangesWithPreviews} from '../previews';
import {commitByHash} from '../serverAPIState';
import {
  commitInfoViewCurrentCommits,
  commitMode,
  latestCommitMessageFieldsWithEdits,
} from './CommitInfoState';
import {Button} from 'isl-components/Button';
import {ErrorNotice} from 'isl-components/ErrorNotice';
import {Icon} from 'isl-components/Icon';
import {TextArea} from 'isl-components/TextArea';
import {Tooltip} from 'isl-components/Tooltip';
import {atom, useAtom, useAtomValue, useSetAtom} from 'jotai';
import {useCallback} from 'react';
import {ComparisonType} from 'shared/Comparison';
import {useThrottledEffect} from 'shared/hooks';
import {randomId, nullthrows} from 'shared/utils';

import './GenerateWithAI.css';

/** Either a commit hash or "commit/aaaaa" when making a new commit on top of hash aaaaa  */
type HashKey = `commit/${string}` | string;

export function GenerateAICommitMessageButton({
  textAreaRef,
  appendToTextArea,
}: {
  textAreaRef: RefObject<HTMLTextAreaElement>;
  appendToTextArea: (toAdd: string) => unknown;
}) {
  const currentCommit = useAtomValue(commitInfoViewCurrentCommits)?.[0];
  const mode = useAtomValue(commitMode);
  const featureEnabled = useFeatureFlagSync(Internal.featureFlags?.GeneratedAICommitMessages);

  const hashKey: HashKey | undefined =
    currentCommit == null
      ? undefined
      : mode === 'commit'
      ? `commit/${currentCommit.hash}`
      : currentCommit.hash;

  useThrottledEffect(
    () => {
      if (currentCommit != null && featureEnabled && hashKey != null) {
        FunnelTracker.get(hashKey)?.track(GeneratedMessageTrackEventName.ButtonImpression);
      }
    },
    100,
    [hashKey, featureEnabled],
  );

  const onDismiss = useCallback(() => {
    if (hashKey != null) {
      const hasAcceptedState = readAtom(hasAcceptedAIMessageSuggestion(hashKey));
      if (hasAcceptedState === true) {
        return;
      }
      FunnelTracker.get(hashKey)?.track(GeneratedMessageTrackEventName.Dismiss);
    }
  }, [hashKey]);

  if (hashKey == null || !featureEnabled) {
    return null;
  }
  return (
    <span key="generate-ai-commit-message-button">
      <Tooltip
        trigger="click"
        placement="bottom"
        component={(dismiss: () => void) => (
          <GenerateAICommitMessageModal
            dismiss={dismiss}
            hashKey={hashKey}
            textArea={textAreaRef.current}
            appendToTextArea={appendToTextArea}
          />
        )}
        onDismiss={onDismiss}
        title={t('Generate a commit message suggestion with AI')}>
        <Button icon data-testid="generate-commit-message-button">
          <Icon icon="sparkle" />
        </Button>
      </Tooltip>
    </span>
  );
}

const cachedSuggestions = new Map<
  string,
  {lastFetch: number; messagePromise: Promise<Result<string>>}
>();
const ONE_HOUR = 60 * 60 * 1000;
const MAX_SUGGESTION_CACHE_AGE = 24 * ONE_HOUR; // cache aggressively since we have an explicit button to invalidate
const generatedCommitMessages = atomFamilyWeak((hashKey: string | undefined) =>
  atomLoadableWithRefresh((get): Promise<Result<string>> => {
    if (hashKey == null || Internal.generateAICommitMessage == null) {
      return Promise.resolve({value: ''});
    }

    const cached = cachedSuggestions.get(hashKey);
    if (cached && Date.now() - cached.lastFetch < MAX_SUGGESTION_CACHE_AGE) {
      return cached.messagePromise;
    }

    const fileChanges = [];
    if (hashKey === 'head') {
      const uncommittedChanges = get(uncommittedChangesWithPreviews);
      fileChanges.push(...uncommittedChanges.slice(0, 10).map(change => change.path));
    } else {
      const commit = get(commitByHash(hashKey));
      if (commit?.isDot) {
        const uncommittedChanges = get(uncommittedChangesWithPreviews);
        fileChanges.push(...uncommittedChanges.slice(0, 10).map(change => change.path));
      }
      fileChanges.push(...(commit?.filesSample.slice(0, 10).map(change => change.path) ?? []));
    }

    const hashOrHead = hashKey.startsWith('commit/') ? 'head' : hashKey;
    const latestFields = readAtom(latestCommitMessageFieldsWithEdits(hashOrHead));
    const latestWrittenTitle = latestFields.Title as string;

    // Note: we don't use the FunnelTracker because this event is not needed for funnel analysis,
    // only for our own duration / error rate tracking.
    const resultPromise = tracker.operation(
      'GenerateAICommitMessage',
      'FetchError',
      {},
      async () => {
        const comparison: Comparison = hashKey.startsWith('commit/')
          ? {type: ComparisonType.UncommittedChanges}
          : {type: ComparisonType.Committed, hash: hashKey};
        const response = await nullthrows(Internal.generateAICommitMessage)({
          comparison,
          title: latestWrittenTitle,
        });

        return response;
      },
    );

    cachedSuggestions.set(hashKey, {
      lastFetch: Date.now(),
      messagePromise: resultPromise,
    });

    return resultPromise;
  }),
);

const hasAcceptedAIMessageSuggestion = atomFamilyWeak((_key: HashKey) => atom<boolean>(false));

function GenerateAICommitMessageModal({
  hashKey,
  dismiss,
  appendToTextArea,
}: {
  hashKey: HashKey;
  textArea: HTMLElement | null;
  dismiss: () => unknown;
  appendToTextArea: (toAdd: string) => unknown;
}) {
  const [content, refetch] = useAtom(generatedCommitMessages(hashKey));

  const setHasAccepted = useSetAtom(hasAcceptedAIMessageSuggestion(hashKey));

  const error =
    content.state === 'hasError'
      ? (content.error as Error)
      : content.state === 'hasData'
      ? (content.data.error as Error)
      : undefined;
  const suggestionId = FunnelTracker.suggestionIdForHashKey(hashKey);

  useThrottledEffect(
    () => {
      FunnelTracker.get(hashKey)?.track(GeneratedMessageTrackEventName.SuggestionRequested);
    },
    100,
    [suggestionId], // ensure we track again if the hash key hasn't changed but a new suggestionID was generated
  );

  useThrottledEffect(
    () => {
      if (content.state === 'hasData' && content.data.value != null) {
        FunnelTracker.get(hashKey)?.track(GeneratedMessageTrackEventName.ResponseImpression);
      }
    },
    100,
    [hashKey, content],
  );

  return (
    <div className="generated-ai-commit-message-modal">
      <Button icon className="dismiss-modal" onClick={dismiss}>
        <Icon icon="x" />
      </Button>
      <b>Generate Summary</b>
      {error ? (
        <ErrorNotice error={error} title={t('Unable to generate commit message')}></ErrorNotice>
      ) : (
        <div className="generated-message-textarea-container">
          <TextArea
            readOnly
            value={content.state === 'hasData' ? content.data.value ?? '' : ''}
            rows={14}
          />
          {content.state === 'loading' && <Icon icon="loading" />}
        </div>
      )}
      <div className="generated-message-button-bar">
        <Button
          disabled={content.state === 'loading' || error != null}
          onClick={() => {
            FunnelTracker.get(hashKey)?.track(GeneratedMessageTrackEventName.RetryClick);
            cachedSuggestions.delete(hashKey); // make sure we don't re-use cached value
            setHasAccepted(false);
            FunnelTracker.restartFunnel(hashKey);
            refetch();
          }}>
          <Icon icon="refresh" />
          <T>Try Again</T>
        </Button>
        <Button
          primary
          disabled={content.state === 'loading' || error != null}
          onClick={() => {
            const value = content.state === 'hasData' ? content.data.value : null;
            if (value) {
              appendToTextArea(value);
            }
            FunnelTracker.get(hashKey)?.track(GeneratedMessageTrackEventName.InsertClick);
            setHasAccepted(true);
            dismiss();
          }}>
          <Icon icon="check" />
          <T>Insert into Summary</T>
        </Button>
      </div>
    </div>
  );
}

export enum FunnelEvent {
  Opportunity = 'opportunity',
  Shown = 'shown',
  Accepted = 'accepted',
  Rejected = 'rejected',
}
export enum GeneratedMessageTrackEventName {
  ButtonImpression = 'generate_button_impression',
  SuggestionRequested = 'suggestion_requested',
  ResponseImpression = 'response_impression',
  InsertClick = 'insert_button_click',
  RetryClick = 'retry_button_click',
  Dismiss = 'dismiss_button_click',
}

/**
 * Manage tracking events and including a suggestion identifier according to the analytics funnel:
 *
 * (O) Opporunity - The dropdown has rendered and a suggestion has begun being rendered
 * (S) Shown - A complete suggestion has been rendered
 * (A) Accepted - The suggestion was accepted
 * (R) Rejected - The suggestion was rejected, retried, or dismissed
 *
 * Each funnel instance has a unique suggestion identifier associated with it.
 * We should log at most one funnel action per suggestion identifier.
 * We still log all events, but if the funnel action has already happened for this suggestion id,
 * we log the funnel event name as undefined.
 *
 * Since it's possible to have multiple suggestions generated for different commits simultaneously,
 * there is one FunnelTracker per funnel / hashKey / suggestion identifier, indexed by HashKey.
 *
 * Note: After retrying a suggestion, we destroy the FunnelTracker so that it is recreated with a new
 * suggestion identifier, aka acts as a new funnel entirely from then on.
 */
class FunnelTracker {
  static trackersByHashKey = new Map<string, FunnelTracker>();

  /** Get or create the funnel tracker for this hashKey */
  static get(hashKey: HashKey): FunnelTracker {
    if (this.trackersByHashKey.has(hashKey)) {
      return nullthrows(this.trackersByHashKey.get(hashKey));
    }
    const tracker = new FunnelTracker();
    this.trackersByHashKey.set(hashKey, tracker);
    return tracker;
  }

  static suggestionIdForHashKey(hashKey: HashKey): string {
    const tracker = FunnelTracker.get(hashKey);
    return tracker.suggestionId;
  }

  /** Restart the funnel for a given `hashKey`, so it generates a new suggestion identifier  */
  static restartFunnel(hashKey: HashKey): void {
    this.trackersByHashKey.delete(hashKey);
  }

  /** Reset internal storage, useful for resetting between tests */
  static resetAllState() {
    this.trackersByHashKey.clear();
  }

  private alreadyTrackedFunnelEvents = new Set<FunnelEvent>();
  private suggestionId = randomId();

  public track(eventName: GeneratedMessageTrackEventName) {
    let funnelEventName: FunnelEvent | undefined = this.mapToFunnelEvent(eventName);
    if (funnelEventName != null && !this.alreadyTrackedFunnelEvents.has(funnelEventName)) {
      // prevent tracking this funnel event again for this suggestion ID
      this.alreadyTrackedFunnelEvents.add(funnelEventName);
    } else {
      funnelEventName = undefined;
    }

    // log all events into the same event, which can be extracted for funnel analysis
    Internal?.trackerWithUserInfo?.track('GenerateAICommitMessageFunnelEvent', {
      extras: {
        eventName,
        suggestionIdentifier: this.suggestionId,
        funnelEventName,
      },
    });
  }

  /** Convert from our internal names to the funnel event names */
  private mapToFunnelEvent(eventName: GeneratedMessageTrackEventName): FunnelEvent | undefined {
    switch (eventName) {
      case GeneratedMessageTrackEventName.ButtonImpression:
        return undefined;
      case GeneratedMessageTrackEventName.SuggestionRequested:
        return FunnelEvent.Opportunity;
      case GeneratedMessageTrackEventName.ResponseImpression:
        return FunnelEvent.Shown;
      case GeneratedMessageTrackEventName.InsertClick:
        return FunnelEvent.Accepted;
      case GeneratedMessageTrackEventName.RetryClick:
        return FunnelEvent.Rejected;
      case GeneratedMessageTrackEventName.Dismiss:
        return FunnelEvent.Rejected;
    }
  }
}

export const __TEST__ = {
  FunnelTracker,
};
