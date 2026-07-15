// Command room state management.

export type CommandMessageRole = 'user' | 'assistant' | 'system';

export interface CommandMessage {
  id: string;
  role: CommandMessageRole;
  text: string;
  timestamp: number;
  /** Action cards attached to this message (from confirm_required frames). */
  actionCards: ActionCard[];
}

export interface ActionCard {
  refId: string;
  summary: string;
  tierReason: string;
  paperLive: 'paper' | 'live';
  requiresTotp: boolean;
  confirmed: boolean;
}

export interface CommandRoomState {
  messages: CommandMessage[];
  streaming: boolean;
  /** Current streaming message being built. */
  streamBuffer: string;
  /** Available slash commands from MCP inventory. */
  slashCommands: Array<{ command: string; description: string }>;
  /** Server-reported session tier. */
  tier: number;
}

export function createCommandRoomState(): CommandRoomState {
  return {
    messages: [],
    streaming: false,
    streamBuffer: '',
    slashCommands: [],
    tier: 1,
  };
}

/** Append assistant text from a streamed command_result frame. */
export function appendStreamChunk(state: CommandRoomState, chunk: string): CommandRoomState {
  return { ...state, streamBuffer: state.streamBuffer + chunk, streaming: true };
}

/** Finalize the current stream into a message. */
export function finalizeStream(state: CommandRoomState): CommandRoomState {
  if (state.streamBuffer.length === 0) return state;
  const message: CommandMessage = {
    id: crypto.randomUUID(),
    role: 'assistant',
    text: state.streamBuffer,
    timestamp: Date.now(),
    actionCards: [],
  };
  return {
    ...state,
    messages: [...state.messages, message],
    streamBuffer: '',
    streaming: false,
  };
}

/** Add an action card from a confirm_required frame. */
export function addActionCard(state: CommandRoomState, card: ActionCard): CommandRoomState {
  const messages = [...state.messages];
  const lastMsg = messages[messages.length - 1];
  if (lastMsg && lastMsg.role === 'assistant') {
    lastMsg.actionCards = [...lastMsg.actionCards, card];
    messages[messages.length - 1] = { ...lastMsg };
  }
  return { ...state, messages };
}

/** Submit a user command. */
export function submitCommand(state: CommandRoomState, text: string, roomContext: Record<string, unknown> = {}): CommandRoomState {
  const userMsg: CommandMessage = {
    id: crypto.randomUUID(),
    role: 'user',
    text,
    timestamp: Date.now(),
    actionCards: [],
  };
  return {
    ...state,
    messages: [...state.messages, userMsg],
    streamBuffer: '',
    streaming: true,
  };
}

/** Update tier from server. */
export function updateTier(state: CommandRoomState, tier: number): CommandRoomState {
  return { ...state, tier };
}
