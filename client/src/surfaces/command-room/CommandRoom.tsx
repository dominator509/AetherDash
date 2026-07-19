// Command room surface (SPEC-004 Surface 5).
// Chat-style interface with streaming assistant text and action cards.
// INV-1: assistant text NEVER auto-executes. Only action cards can trigger confirms.

import { useState, useCallback, useRef, useEffect } from "react";
import {
  createCommandRoomState,
  submitCommand,
  appendStreamChunk,
  finalizeStream,
  addActionCard,
  CommandRoomState,
  CommandMessage,
  ActionCard,
} from "../../state/command-room";
import {
  formatDecisionPacket,
  launchSwarm,
  type SwarmLaunchRequest,
  type SwarmProgressEvent,
} from "../../lib/mcp";

interface CommandRoomProps {
  gatewayUrl?: string;
  sessionToken?: string;
}

export function CommandRoom({ gatewayUrl, sessionToken }: CommandRoomProps = {}) {
  const [state, setState] = useState<CommandRoomState>(createCommandRoomState());
  const [input, setInput] = useState("");
  const [roomContext] = useState({ surface: "command-room", timestamp: Date.now() });
  const messagesEnd = useRef<HTMLDivElement>(null);
  const pendingSwarms = useRef(new Map<string, SwarmLaunchRequest>());

  const scrollToBottom = () => messagesEnd.current?.scrollIntoView({ behavior: "smooth" });
  useEffect(scrollToBottom, [state.messages, state.streamBuffer]);

  const appendAssistant = useCallback((text: string) => {
    setState((current) => finalizeStream(appendStreamChunk(current, text)));
  }, []);

  const appendProgress = useCallback((event: SwarmProgressEvent) => {
    const subject = event.worker_id ? `${event.worker_id}: ` : "";
    const detail = event.detail ? ` (${event.detail})` : "";
    setState((current) =>
      appendStreamChunk(current, `${subject}${event.kind.replaceAll("_", " ")}${detail}\n`),
    );
  }, []);

  const handleSubmit = useCallback(async () => {
    if (!input.trim()) return;
    const command = input.trim();
    setState((s) => submitCommand(s, command, roomContext));
    setInput("");
    const prefix = "/swarm.launch ";
    if (!command.startsWith(prefix)) {
      appendAssistant("Unknown command. Use /swarm.launch followed by a research question.");
      return;
    }
    if (!gatewayUrl || !sessionToken) {
      appendAssistant(
        "Swarm launch is unavailable because the authenticated gateway is not configured.",
      );
      return;
    }
    const request: SwarmLaunchRequest = {
      question: command.slice(prefix.length).trim(),
      budget: {
        max_calls: 3,
        max_tokens: 12_000,
        max_cost_usd: "1.00",
        max_seconds: 60,
      },
      context: roomContext,
      workers: 3,
    };
    try {
      const result = await launchSwarm(
        gatewayUrl,
        sessionToken,
        request,
        undefined,
        appendProgress,
      );
      if (result.status === "completed") {
        appendAssistant(formatDecisionPacket(result.packet));
        return;
      }
      pendingSwarms.current.set(result.confirmationRef, request);
      appendAssistant(
        "This bounded research swarm requires your confirmation before it spends its declared budget.",
      );
      setState((current) =>
        addActionCard(current, {
          refId: result.confirmationRef,
          summary: `Launch research swarm: ${request.question}`,
          tierReason: "Tier 3 budgeted action; packet is proposal-only",
          paperLive: "paper",
          requiresTotp: false,
          confirmed: false,
        }),
      );
    } catch (error) {
      appendAssistant(error instanceof Error ? error.message : "Swarm launch failed.");
    }
  }, [appendAssistant, appendProgress, gatewayUrl, input, roomContext, sessionToken]);

  const confirmSwarm = useCallback(
    async (refId: string) => {
      const request = pendingSwarms.current.get(refId);
      if (!request || !gatewayUrl || !sessionToken) {
        throw new Error("Swarm confirmation is unavailable or expired.");
      }
      const result = await launchSwarm(gatewayUrl, sessionToken, request, refId, appendProgress);
      if (result.status !== "completed") {
        throw new Error("Swarm confirmation did not complete.");
      }
      pendingSwarms.current.delete(refId);
      appendAssistant(formatDecisionPacket(result.packet));
    },
    [appendAssistant, appendProgress, gatewayUrl, sessionToken],
  );

  return (
    <div className="flex flex-col h-full">
      {/* Tier badge */}
      <div className="flex items-center justify-between px-4 py-2 bg-gray-50 border-b">
        <h2 className="font-semibold">Command Room</h2>
        <span
          className={`px-2 py-0.5 rounded text-xs font-bold ${state.tier >= 4 ? "bg-purple-100 text-purple-800" : "bg-gray-200 text-gray-600"}`}
        >
          Tier {state.tier}
        </span>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto p-4 space-y-3">
        {state.messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} onConfirm={confirmSwarm} />
        ))}
        {state.streaming && (
          <div className="p-3 bg-blue-50 rounded text-blue-800 animate-pulse">
            {state.streamBuffer || "Thinking..."}
          </div>
        )}
        <div ref={messagesEnd} />
      </div>

      {/* Input */}
      <div className="border-t p-3">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            handleSubmit();
          }}
          className="flex gap-2"
        >
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Type a command or / for slash commands..."
            className="flex-1 px-3 py-2 border rounded focus:outline-none focus:ring-2 focus:ring-blue-400"
            disabled={state.streaming}
          />
          <button
            type="submit"
            disabled={state.streaming || !input.trim()}
            className="px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50"
          >
            Send
          </button>
        </form>
      </div>
    </div>
  );
}

function MessageBubble({
  message,
  onConfirm,
}: {
  message: CommandMessage;
  onConfirm: (refId: string) => Promise<void>;
}) {
  const isUser = message.role === "user";
  const isSystem = message.role === "system";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-2xl rounded-lg p-3 ${isUser ? "bg-blue-600 text-white" : isSystem ? "bg-yellow-50 text-yellow-800 border border-yellow-200" : "bg-gray-100 text-gray-900"}`}
      >
        <p className="whitespace-pre-wrap">{message.text}</p>
        {message.actionCards.length > 0 && (
          <div className="mt-3 space-y-2">
            {message.actionCards.map((card) => (
              <ActionCardComponent key={card.refId} card={card} onConfirm={onConfirm} />
            ))}
          </div>
        )}
        <span className="text-xs opacity-50 mt-1 block">
          {new Date(message.timestamp).toLocaleTimeString()}
        </span>
      </div>
    </div>
  );
}

function ActionCardComponent({
  card,
  onConfirm,
}: {
  card: ActionCard;
  onConfirm: (refId: string) => Promise<void>;
}) {
  const [confirmed, setConfirmed] = useState(card.confirmed);
  const [totp, setTotp] = useState("");

  const [error, setError] = useState<string | null>(null);

  const handleConfirm = async () => {
    if (card.requiresTotp && !totp.trim()) return;
    try {
      await onConfirm(card.refId);
      setConfirmed(true);
      setError(null);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Confirmation failed.");
    }
  };

  return (
    <div
      className={`border rounded p-3 ${confirmed ? "bg-green-50 border-green-200" : "bg-white border-gray-200"}`}
    >
      <div className="flex items-center justify-between mb-2">
        <span className="font-medium text-sm">{card.summary}</span>
        <span
          className={`text-xs px-2 py-0.5 rounded ${card.paperLive === "live" ? "bg-red-100 text-red-700" : "bg-blue-100 text-blue-700"}`}
        >
          {card.paperLive.toUpperCase()}
        </span>
      </div>
      <p className="text-xs text-gray-500 mb-2">{card.tierReason}</p>
      {card.requiresTotp && !confirmed && (
        <input
          type="text"
          value={totp}
          onChange={(e) => setTotp(e.target.value)}
          placeholder="TOTP code"
          className="w-full px-2 py-1 text-sm border rounded mb-2"
          maxLength={6}
        />
      )}
      {!confirmed && (
        <button
          onClick={handleConfirm}
          className="px-3 py-1 text-sm bg-blue-600 text-white rounded hover:bg-blue-700"
        >
          Confirm
        </button>
      )}
      {confirmed && <span className="text-sm text-green-600 font-medium">✓ Confirmed</span>}
      {error && <p className="mt-2 text-xs text-red-600">{error}</p>}
    </div>
  );
}
