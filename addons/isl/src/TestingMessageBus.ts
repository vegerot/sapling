/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {MessageBus, MessageBusStatus} from './MessageBus';
import type {Disposable} from './types';

/** This fake implementation of MessageBus expects you to manually simulate messages from the server */
export class TestingEventBus implements MessageBus {
  public handlers: Array<(e: MessageEvent<string>) => void> = [];
  public sent: Array<string> = [];
  onMessage(handler: (event: MessageEvent<string>) => void | Promise<void>): Disposable {
    this.handlers.push(handler);
    return {dispose: () => {}};
  }

  postMessage(message: string) {
    this.sent.push(message);
  }

  public statusChangeHandlers = new Set<(status: MessageBusStatus) => unknown>();
  onChangeStatus(handler: (status: MessageBusStatus) => unknown): Disposable {
    // pretend connection opens immediately
    handler({type: 'open'});
    this.statusChangeHandlers.add(handler);

    return {
      dispose: () => {
        this.statusChangeHandlers.delete(handler);
      },
    };
  }

  // additional methods for testing

  simulateMessage(message: string) {
    this.handlers.forEach(handle => handle({data: message} as MessageEvent<string>));
  }

  resetTestMessages() {
    this.sent = [];
    // Emulate reconnect to trigger serverAPI.onSetup callbacks.
    this.simulateServerStatusChange({type: 'reconnecting'});
    this.simulateServerStatusChange({type: 'open'});
  }

  simulateServerStatusChange(newStatus: MessageBusStatus) {
    for (const handler of this.statusChangeHandlers) {
      handler(newStatus);
    }
  }
}
