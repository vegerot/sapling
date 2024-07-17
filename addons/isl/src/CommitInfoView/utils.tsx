/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {CommitInfo} from '../types';
import type {CommitMessageFields, FieldConfig, FieldsBeingEdited} from './types';
import type {ReactNode} from 'react';

import {InlineBadge} from '../InlineBadge';
import {YouAreHereLabel} from '../YouAreHereLabel';
import {t, T} from '../i18n';
import platform from '../platform';
import {RelativeDate} from '../relativeDate';
import {Subtle} from 'isl-components/Subtle';
import {Tooltip} from 'isl-components/Tooltip';

export function CommitTitleByline({commit}: {commit: CommitInfo}) {
  const createdByInfo = (
    // TODO: determine if you're the author to say "you"
    <T replace={{$author: commit.author}}>Created by $author</T>
  );
  return (
    <Subtle className="commit-info-title-byline">
      {commit.isDot ? <YouAreHereLabel /> : null}
      {commit.phase === 'public' ? <PublicCommitBadge /> : null}
      <OverflowEllipsis shrink>
        <Tooltip trigger="hover" component={() => createdByInfo}>
          {createdByInfo}
        </Tooltip>
      </OverflowEllipsis>
      <OverflowEllipsis>
        <Tooltip trigger="hover" title={commit.date.toLocaleString()}>
          <RelativeDate date={commit.date} />
        </Tooltip>
      </OverflowEllipsis>
    </Subtle>
  );
}

function PublicCommitBadge() {
  return (
    <Tooltip
      placement="bottom"
      title={t(
        "This commit has already been pushed to an append-only remote branch and can't be modified locally.",
      )}>
      <InlineBadge>
        <T>Public</T>
      </InlineBadge>
    </Tooltip>
  );
}

export function OverflowEllipsis({children, shrink}: {children: ReactNode; shrink?: boolean}) {
  return <div className={`overflow-ellipsis${shrink ? ' overflow-shrink' : ''}`}>{children}</div>;
}

export function SmallCapsTitle({children}: {children: ReactNode}) {
  return <div className="commit-info-small-title">{children}</div>;
}

export function Section({
  children,
  className,
  ...rest
}: React.DetailedHTMLProps<React.HTMLAttributes<HTMLElement>, HTMLElement>) {
  return (
    <section {...rest} className={'commit-info-section' + (className ? ' ' + className : '')}>
      {children}
    </section>
  );
}

export function getFieldToAutofocus(
  fields: Array<FieldConfig>,
  fieldsBeingEdited: FieldsBeingEdited,
  lastFieldsBeingEdited: FieldsBeingEdited | undefined,
): keyof CommitMessageFields | undefined {
  for (const field of fields) {
    const isNewlyBeingEdited =
      fieldsBeingEdited[field.key] &&
      (lastFieldsBeingEdited == null || !lastFieldsBeingEdited[field.key]);
    if (isNewlyBeingEdited) {
      return field.key;
    }
  }
  return undefined;
}

export function getOnClickToken(
  field: FieldConfig & {type: 'field'},
): ((token: string) => unknown) | undefined {
  if (field.getUrl == null) {
    return undefined;
  }

  return token => {
    const url = field.getUrl?.(token);
    url && platform.openExternalLink(url);
  };
}
