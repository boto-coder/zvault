/**
 * Nostr relay WebSocket client for the browser extension.
 *
 * Uses the native WebSocket API (available in service workers and content scripts).
 * Implements NIP-01 client-to-relay protocol: EVENT, REQ, CLOSE, OK, EOSE.
 *
 * Features:
 * - Publish signed events to a relay
 * - Subscribe with NIP-01 filters
 * - Automatic reconnection with exponential backoff
 * - EOSE (end of stored events) handling
 */

export interface SubscriptionFilter {
  kinds?: number[];
  authors?: string[];
  "#p"?: string[];
  since?: number;
  until?: number;
  limit?: number;
}

export type RelayEventCallback = (event: NostrEventJson) => void;
export type RelayEoseCallback = (subscriptionId: string) => void;
export type RelayStatusCallback = (status: RelayStatus) => void;

export interface NostrEventJson {
  id: string;
  pubkey: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
  sig: string;
}

export type RelayStatus = "connecting" | "connected" | "disconnected" | "error";

interface PendingPublish {
  resolve: () => void;
  reject: (err: Error) => void;
  timeout: ReturnType<typeof setTimeout>;
}

interface ActiveSubscription {
  id: string;
  filter: SubscriptionFilter;
  onEvent: RelayEventCallback;
  onEose?: RelayEoseCallback;
}

/**
 * WebSocket client for a single Nostr relay.
 */
export class RelayClient {
  private url: string;
  private ws: WebSocket | null = null;
  private status: RelayStatus = "disconnected";
  private subscriptions: Map<string, ActiveSubscription> = new Map();
  private pendingPublishes: Map<string, PendingPublish> = new Map();
  private subCounter = 0;
  private reconnectAttempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private maxReconnectAttempts = 10;
  private shouldReconnect = true;
  private statusCallback: RelayStatusCallback | null = null;

  constructor(url: string) {
    this.url = url;
  }

  /**
   * Set a callback for relay status changes.
   */
  onStatusChange(callback: RelayStatusCallback): void {
    this.statusCallback = callback;
  }

  /**
   * Get the current relay status.
   */
  getStatus(): RelayStatus {
    return this.status;
  }

  /**
   * Get the relay URL.
   */
  getUrl(): string {
    return this.url;
  }

  /**
   * Connect to the relay. Resolves when the WebSocket is open.
   */
  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        resolve();
        return;
      }

      this.shouldReconnect = true;
      this.setStatus("connecting");

      try {
        this.ws = new WebSocket(this.url);
      } catch (err) {
        this.setStatus("error");
        reject(new Error(`Failed to create WebSocket: ${err}`));
        return;
      }

      this.ws.onopen = () => {
        this.reconnectAttempts = 0;
        this.setStatus("connected");
        // Re-subscribe any active subscriptions after reconnect
        this.resubscribeAll();
        resolve();
      };

      this.ws.onclose = () => {
        this.setStatus("disconnected");
        this.cleanupPendingPublishes("relay connection closed");
        if (this.shouldReconnect) {
          this.scheduleReconnect();
        }
      };

      this.ws.onerror = () => {
        this.setStatus("error");
        reject(new Error(`WebSocket error connecting to ${this.url}`));
      };

      this.ws.onmessage = (msgEvent: MessageEvent) => {
        this.handleMessage(msgEvent.data as string);
      };
    });
  }

  /**
   * Disconnect from the relay. Does not attempt reconnection.
   */
  disconnect(): void {
    this.shouldReconnect = false;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.subscriptions.clear();
    this.cleanupPendingPublishes("disconnected");
    if (this.ws) {
      this.ws.onclose = null;
      this.ws.onerror = null;
      this.ws.onmessage = null;
      if (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING) {
        this.ws.close();
      }
      this.ws = null;
    }
    this.setStatus("disconnected");
  }

  /**
   * Publish a signed Nostr event to the relay.
   * Resolves when the relay sends an OK response, rejects on failure or timeout.
   */
  publish(event: NostrEventJson): Promise<void> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        reject(new Error("Not connected to relay"));
        return;
      }

      const msg = JSON.stringify(["EVENT", event]);
      const timeout = setTimeout(() => {
        this.pendingPublishes.delete(event.id);
        reject(new Error("Publish timeout (10s)"));
      }, 10_000);

      this.pendingPublishes.set(event.id, { resolve, reject, timeout });
      this.ws.send(msg);
    });
  }

  /**
   * Subscribe to events matching a filter.
   * Returns the subscription ID. Events are delivered via the onEvent callback.
   */
  subscribe(
    filter: SubscriptionFilter,
    onEvent: RelayEventCallback,
    onEose?: RelayEoseCallback,
  ): string {
    this.subCounter += 1;
    const subId = `sub_${this.subCounter}`;

    const sub: ActiveSubscription = { id: subId, filter, onEvent, onEose };
    this.subscriptions.set(subId, sub);

    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      const msg = JSON.stringify(["REQ", subId, filter]);
      this.ws.send(msg);
    }

    return subId;
  }

  /**
   * Close a subscription by ID.
   */
  unsubscribe(subscriptionId: string): void {
    this.subscriptions.delete(subscriptionId);
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      const msg = JSON.stringify(["CLOSE", subscriptionId]);
      this.ws.send(msg);
    }
  }

  // ─── Private ─────────────────────────────────────────────────────────────

  private setStatus(status: RelayStatus): void {
    this.status = status;
    this.statusCallback?.(status);
  }

  private handleMessage(data: string): void {
    let parsed: unknown[];
    try {
      parsed = JSON.parse(data) as unknown[];
    } catch {
      console.warn("[RelayClient] Failed to parse relay message:", data);
      return;
    }

    if (!Array.isArray(parsed) || parsed.length < 2) return;

    const type = parsed[0] as string;

    switch (type) {
      case "EVENT": {
        // ["EVENT", sub_id, event]
        if (parsed.length < 3) return;
        const subId = parsed[1] as string;
        const event = parsed[2] as NostrEventJson;
        const sub = this.subscriptions.get(subId);
        if (sub) {
          sub.onEvent(event);
        }
        break;
      }
      case "OK": {
        // ["OK", event_id, success, message]
        if (parsed.length < 4) return;
        const eventId = parsed[1] as string;
        const success = parsed[2] as boolean;
        const message = parsed[3] as string;
        const pending = this.pendingPublishes.get(eventId);
        if (pending) {
          clearTimeout(pending.timeout);
          this.pendingPublishes.delete(eventId);
          if (success) {
            pending.resolve();
          } else {
            pending.reject(new Error(`Relay rejected event: ${message}`));
          }
        }
        break;
      }
      case "EOSE": {
        // ["EOSE", sub_id]
        const subId = parsed[1] as string;
        const sub = this.subscriptions.get(subId);
        if (sub?.onEose) {
          sub.onEose(subId);
        }
        break;
      }
      case "NOTICE": {
        // ["NOTICE", message]
        console.debug("[RelayClient] NOTICE:", parsed[1]);
        break;
      }
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.warn(`[RelayClient] Max reconnect attempts reached for ${this.url}`);
      return;
    }

    // Exponential backoff: 1s, 2s, 4s, 8s, 16s, 32s, 60s (capped)
    const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 60_000);
    this.reconnectAttempts += 1;

    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (this.shouldReconnect) {
        this.connect().catch(() => {
          // Reconnection failed, will be retried by onclose handler
        });
      }
    }, delay);
  }

  private resubscribeAll(): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
    for (const sub of this.subscriptions.values()) {
      const msg = JSON.stringify(["REQ", sub.id, sub.filter]);
      this.ws.send(msg);
    }
  }

  private cleanupPendingPublishes(reason: string): void {
    for (const [eventId, pending] of this.pendingPublishes) {
      clearTimeout(pending.timeout);
      pending.reject(new Error(reason));
      this.pendingPublishes.delete(eventId);
    }
  }
}

/**
 * Manager for multiple relay connections.
 *
 * Provides a unified interface to publish/subscribe across all connected relays.
 */
export class RelayPool {
  private clients: Map<string, RelayClient> = new Map();

  /**
   * Add and connect to a relay.
   */
  async addRelay(url: string): Promise<RelayClient> {
    if (this.clients.has(url)) {
      return this.clients.get(url)!;
    }
    const client = new RelayClient(url);
    this.clients.set(url, client);
    await client.connect();
    return client;
  }

  /**
   * Remove and disconnect from a relay.
   */
  removeRelay(url: string): void {
    const client = this.clients.get(url);
    if (client) {
      client.disconnect();
      this.clients.delete(url);
    }
  }

  /**
   * Disconnect from all relays.
   */
  disconnectAll(): void {
    for (const client of this.clients.values()) {
      client.disconnect();
    }
    this.clients.clear();
  }

  /**
   * Publish an event to all connected relays.
   * Returns the number of successful publishes.
   */
  async publishToAll(event: NostrEventJson): Promise<number> {
    let successCount = 0;
    const promises = Array.from(this.clients.values()).map(async (client) => {
      if (client.getStatus() === "connected") {
        try {
          await client.publish(event);
          successCount += 1;
        } catch (err) {
          console.warn(`[RelayPool] Publish failed to ${client.getUrl()}:`, err);
        }
      }
    });
    await Promise.allSettled(promises);
    return successCount;
  }

  /**
   * Subscribe to events on all connected relays.
   * Returns subscription IDs per relay.
   */
  subscribeAll(
    filter: SubscriptionFilter,
    onEvent: RelayEventCallback,
    onEose?: RelayEoseCallback,
  ): Map<string, string> {
    const subIds = new Map<string, string>();
    for (const [url, client] of this.clients) {
      if (client.getStatus() === "connected") {
        const subId = client.subscribe(filter, onEvent, onEose);
        subIds.set(url, subId);
      }
    }
    return subIds;
  }

  /**
   * Unsubscribe from all relays.
   */
  unsubscribeAll(subIds: Map<string, string>): void {
    for (const [url, subId] of subIds) {
      const client = this.clients.get(url);
      if (client) {
        client.unsubscribe(subId);
      }
    }
  }

  /**
   * Get all connected relay URLs.
   */
  getConnectedRelays(): string[] {
    return Array.from(this.clients.entries())
      .filter(([, client]) => client.getStatus() === "connected")
      .map(([url]) => url);
  }
}
