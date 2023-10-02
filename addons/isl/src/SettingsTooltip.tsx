/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {ThemeColor} from './theme';
import type {PreferredSubmitCommand} from './types';
import type {ReactNode} from 'react';

import {confirmShouldSubmitEnabledAtom} from './ConfirmSubmitStack';
import {DropdownField, DropdownFields} from './DropdownFields';
import {Tooltip} from './Tooltip';
import {codeReviewProvider} from './codeReview/CodeReviewInfo';
import {showDiffNumberConfig} from './codeReview/DiffBadge';
import {SubmitAsDraftCheckbox} from './codeReview/DraftCheckbox';
import {debugToolsEnabledState} from './debug/DebugToolsState';
import {t, T} from './i18n';
import {SetConfigOperation} from './operations/SetConfigOperation';
import platform from './platform';
import {repositoryInfo, useRunOperation} from './serverAPIState';
import {themeState} from './theme';
import {
  VSCodeButton,
  VSCodeCheckbox,
  VSCodeDropdown,
  VSCodeLink,
  VSCodeOption,
} from '@vscode/webview-ui-toolkit/react';
import {useRecoilState, useRecoilValue} from 'recoil';
import {Icon} from 'shared/Icon';
import {unwrap} from 'shared/utils';

import './SettingsTooltip.css';

export function SettingsGearButton() {
  return (
    <Tooltip trigger="click" component={() => <SettingsDropdown />} placement="bottom">
      <VSCodeButton appearance="icon" data-testid="settings-gear-button">
        <Icon icon="gear" />
      </VSCodeButton>
    </Tooltip>
  );
}

function SettingsDropdown() {
  const [theme, setTheme] = useRecoilState(themeState);
  const [repoInfo, setRepoInfo] = useRecoilState(repositoryInfo);
  const runOperation = useRunOperation();
  const [showDiffNumber, setShowDiffNumber] = useRecoilState(showDiffNumberConfig);
  return (
    <DropdownFields title={<T>Settings</T>} icon="gear" data-testid="settings-dropdown">
      {platform.theme != null ? null : (
        <Setting title={<T>Theme</T>}>
          <VSCodeDropdown
            value={theme}
            onChange={event =>
              setTheme(
                (event as React.FormEvent<HTMLSelectElement>).currentTarget.value as ThemeColor,
              )
            }>
            <VSCodeOption value="dark">
              <T>Dark</T>
            </VSCodeOption>
            <VSCodeOption value="light">
              <T>Light</T>
            </VSCodeOption>
          </VSCodeDropdown>
        </Setting>
      )}
      <Setting
        title={<T>Language</T>}
        description={<T>Locale for translations used in the UI. Currently only en supported.</T>}>
        <VSCodeDropdown value="en" disabled>
          <VSCodeOption value="en">en</VSCodeOption>
        </VSCodeDropdown>
      </Setting>
      {repoInfo?.type !== 'success' ? (
        <Icon icon="loading" />
      ) : repoInfo?.codeReviewSystem.type === 'github' ? (
        <Setting
          title={<T>Preferred Code Review Submit Command</T>}
          description={
            <>
              <T>Which command to use to submit code for code review on GitHub.</T>{' '}
              <VSCodeLink
                href="https://sapling-scm.com/docs/git/intro#pull-requests"
                target="_blank">
                <T>Learn More.</T>
              </VSCodeLink>
            </>
          }>
          <VSCodeDropdown
            value={repoInfo.preferredSubmitCommand ?? 'not set'}
            onChange={event => {
              const value = (event as React.FormEvent<HTMLSelectElement>).currentTarget.value as
                | PreferredSubmitCommand
                | 'not set';
              if (value === 'not set') {
                return;
              }

              runOperation(
                new SetConfigOperation('local', 'github.preferred_submit_command', value),
              );
              setRepoInfo(info => ({...unwrap(info), preferredSubmitCommand: value}));
            }}>
            {repoInfo.preferredSubmitCommand == null ? (
              <VSCodeOption value={'not set'}>(not set)</VSCodeOption>
            ) : null}
            <VSCodeOption value="ghstack">sl ghstack</VSCodeOption>
            <VSCodeOption value="pr">sl pr</VSCodeOption>
          </VSCodeDropdown>
        </Setting>
      ) : null}
      <Setting title={<T>Code Review</T>}>
        <div className="multiple-settings">
          <VSCodeCheckbox
            checked={showDiffNumber}
            onChange={e => {
              setShowDiffNumber((e.target as HTMLInputElement).checked);
            }}>
            <T>Show copyable Diff / Pull Request numbers inline for each commit</T>
          </VSCodeCheckbox>
          <ConfirmSubmitStackSetting />
          <SubmitAsDraftCheckbox forceShow />
        </div>
      </Setting>
      <DebugToolsField />
    </DropdownFields>
  );
}

function ConfirmSubmitStackSetting() {
  const [value, setValue] = useRecoilState(confirmShouldSubmitEnabledAtom);
  const provider = useRecoilValue(codeReviewProvider);
  if (provider == null || !provider.supportSubmittingAsDraft) {
    return null;
  }
  return (
    <Tooltip
      title={t(
        'This lets you choose to submit as draft and provide an update message. ' +
          'If false, no confirmation is shown and it will submit as draft if you previously ' +
          'checked the submit as draft checkbox.',
      )}>
      <VSCodeCheckbox
        checked={value}
        onChange={e => {
          setValue((e.target as HTMLInputElement).checked);
        }}>
        <T>Show confirmation when submitting a stack</T>
      </VSCodeCheckbox>
    </Tooltip>
  );
}

function DebugToolsField() {
  const [isDebug, setIsDebug] = useRecoilState(debugToolsEnabledState);

  return (
    <DropdownField title={t('Debug Tools')}>
      <VSCodeCheckbox
        checked={isDebug}
        onChange={e => {
          setIsDebug((e.target as HTMLInputElement).checked);
        }}>
        <T>Enable Debug Tools</T>
      </VSCodeCheckbox>
    </DropdownField>
  );
}

function Setting({
  children,
  title,
  description,
}: {
  children: ReactNode;
  title: ReactNode;
  description?: ReactNode;
}) {
  return (
    <DropdownField title={title}>
      {description && <div className="setting-description">{description}</div>}
      {children}
    </DropdownField>
  );
}
