type Handler = (message: any) => void;
type OpenHandler = () => void;

export function wsUrl(path: string): string {
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${protocol}//${location.host}${path}`;
}

/** A JSON WebSocket with reconnect + a small outbox. */
export class JsonSocket {
  private socket: WebSocket | null = null;
  private outbox: string[] = [];
  private stopped = false;
  private attempt = 0;
  private retryTimer: number | null = null;

  constructor(
    private readonly path: string,
    private readonly onMessage: Handler,
    private readonly onOpen?: OpenHandler,
  ) {
    this.connect();
  }

  private connect(): void {
    const socket = new WebSocket(wsUrl(this.path));
    this.socket = socket;

    socket.onopen = () => {
      this.attempt = 0;
      this.onOpen?.();
      for (const message of this.outbox.splice(0)) {
        socket.send(message);
      }
    };

    socket.onmessage = (event) => {
      try {
        this.onMessage(JSON.parse(event.data as string));
      } catch {
        /* ignore malformed frames */
      }
    };

    socket.onclose = () => {
      this.socket = null;
      if (!this.stopped) this.retry();
    };

    socket.onerror = () => {
      socket.close();
    };
  }

  private retry(): void {
    if (this.retryTimer !== null) return;
    const delay = Math.min(10_000, 500 * 2 ** this.attempt);
    this.attempt += 1;
    this.retryTimer = window.setTimeout(() => {
      this.retryTimer = null;
      this.connect();
    }, delay);
  }

  send(message: unknown): void {
    const text = JSON.stringify(message);
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(text);
    } else {
      this.outbox.push(text);
    }
  }

  close(): void {
    this.stopped = true;
    if (this.retryTimer !== null) window.clearTimeout(this.retryTimer);
    this.socket?.close();
    this.socket = null;
  }
}
