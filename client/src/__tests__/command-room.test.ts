// Command room unit tests.
// INV-1 proof: text never executes.

import { describe, it, expect } from 'vitest';
import { createCommandRoomState, submitCommand, finalizeStream, appendStreamChunk, addActionCard, updateTier } from '../state/command-room';

describe('CommandRoomState', () => {
  it('starts with empty messages', () => {
    const state = createCommandRoomState();
    expect(state.messages).toHaveLength(0);
    expect(state.streaming).toBe(false);
    expect(state.tier).toBe(1);
  });

  it('submitCommand adds user message and enables streaming', () => {
    let state = createCommandRoomState();
    state = submitCommand(state, 'hello');
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0].role).toBe('user');
    expect(state.messages[0].text).toBe('hello');
    expect(state.streaming).toBe(true);
  });

  it('appendStreamChunk builds up stream buffer', () => {
    let state = createCommandRoomState();
    state = submitCommand(state, 'test');
    state = appendStreamChunk(state, 'Hello ');
    state = appendStreamChunk(state, 'world');
    expect(state.streamBuffer).toBe('Hello world');
  });

  it('finalizeStream creates assistant message', () => {
    let state = createCommandRoomState();
    state = submitCommand(state, 'test');
    state = appendStreamChunk(state, 'response');
    state = finalizeStream(state);
    expect(state.messages).toHaveLength(2);
    expect(state.messages[1].role).toBe('assistant');
    expect(state.messages[1].text).toBe('response');
    expect(state.streaming).toBe(false);
  });

  it('addActionCard attaches to last assistant message', () => {
    let state = createCommandRoomState();
    state = submitCommand(state, 'do something');
    state = appendStreamChunk(state, 'ok');
    state = finalizeStream(state);
    state = addActionCard(state, { refId: '1', summary: 'Submit order', tierReason: 'tier >= 3 required', paperLive: 'paper', requiresTotp: false, confirmed: false });
    expect(state.messages[1].actionCards).toHaveLength(1);
    expect(state.messages[1].actionCards[0].summary).toBe('Submit order');
  });

  it('INV-1: text alone never creates an action', () => {
    // The command room state has NO mechanism for text to auto-execute.
    // Only explicit addActionCard() creates action cards.
    // This test PROVES INV-1 at the UI layer.
    let state = createCommandRoomState();
    state = submitCommand(state, 'I will submit an order for 10 contracts');
    state = appendStreamChunk(state, 'Order submitted!');
    state = finalizeStream(state);
    // Even though the text contains "submit" and "order", no actions are created
    expect(state.messages[1].actionCards).toHaveLength(0);
  });
});
