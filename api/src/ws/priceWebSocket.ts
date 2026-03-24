import { WebSocket, WebSocketServer } from 'ws';
import { IncomingMessage } from 'http';
import { Server } from 'http';
import axios from 'axios';
import logger from '../utils/logger';
import { config } from '../config';
import {
  PriceData,
  ClientMessage,
  ServerMessage,
  WsSubscribeMessage,
  WsUnsubscribeMessage,
} from '../types';

const SUPPORTED_ASSETS = ['XLM', 'USDC', 'BTC', 'ETH', 'SOL'];

const COINGECKO_IDS: Record<string, string> = {
  XLM: 'stellar',
  USDC: 'usd-coin',
  BTC: 'bitcoin',
  ETH: 'ethereum',
  SOL: 'solana',
};

export class PriceWebSocketServer {
  private wss: WebSocketServer;
  private clientSubscriptions: Map<WebSocket, Set<string>> = new Map();
  private lastPrices: Map<string, PriceData> = new Map();
  private pollIntervalId?: ReturnType<typeof setInterval>;
  private heartbeatIntervalId?: ReturnType<typeof setInterval>;

  constructor(server: Server) {
    this.wss = new WebSocketServer({ server, path: '/api/ws/prices' });
    this.setupConnectionHandler();
    this.startPricePolling();
    this.startHeartbeat();
    logger.info('WebSocket price server initialised at /api/ws/prices');
  }

  private setupConnectionHandler(): void {
    this.wss.on('connection', (ws: WebSocket, req: IncomingMessage) => {
      logger.info('WebSocket client connected', { ip: req.socket.remoteAddress });
      this.clientSubscriptions.set(ws, new Set());

      ws.on('message', (data) => {
        try {
          const msg: ClientMessage = JSON.parse(data.toString());
          this.handleClientMessage(ws, msg);
        } catch {
          this.send(ws, { type: 'error', message: 'Invalid JSON message' });
        }
      });

      ws.on('close', () => {
        this.clientSubscriptions.delete(ws);
        logger.info('WebSocket client disconnected');
      });

      ws.on('error', (err) => {
        logger.error('WebSocket client error', { error: err.message });
        this.clientSubscriptions.delete(ws);
      });
    });
  }

  private handleClientMessage(ws: WebSocket, msg: ClientMessage): void {
    switch (msg.type) {
      case 'subscribe': {
        const subs = this.clientSubscriptions.get(ws);
        if (!subs) return;

        const requested = (msg as WsSubscribeMessage).assets;
        const toSubscribe = requested.includes('*')
          ? SUPPORTED_ASSETS
          : requested.map((a) => a.toUpperCase()).filter((a) => SUPPORTED_ASSETS.includes(a));

        toSubscribe.forEach((a) => subs.add(a));
        this.send(ws, { type: 'subscribed', assets: toSubscribe });

        // Send cached prices immediately
        toSubscribe.forEach((asset) => {
          const cached = this.lastPrices.get(asset);
          if (cached) {
            this.send(ws, {
              type: 'price_update',
              asset: cached.asset,
              price: cached.price,
              timestamp: cached.timestamp,
            });
          }
        });
        break;
      }

      case 'unsubscribe': {
        const subs = this.clientSubscriptions.get(ws);
        if (!subs) return;
        const assets = (msg as WsUnsubscribeMessage).assets.map((a) => a.toUpperCase());
        assets.forEach((a) => subs.delete(a));
        this.send(ws, { type: 'unsubscribed', assets });
        break;
      }

      case 'ping':
        this.send(ws, { type: 'pong' });
        break;

      default:
        this.send(ws, { type: 'error', message: 'Unknown message type' });
    }
  }

  private send(ws: WebSocket, msg: ServerMessage): void {
    if (ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(msg));
    }
  }

  async fetchPrices(): Promise<Map<string, number>> {
    const prices = new Map<string, number>();

    // Prefer oracle API URL if configured
    const oracleUrl = config.ws.oracleApiUrl;
    if (oracleUrl) {
      try {
        const response = await axios.get<Record<string, number>>(`${oracleUrl}/prices`, {
          timeout: 5000,
        });
        Object.entries(response.data).forEach(([asset, price]) =>
          prices.set(asset.toUpperCase(), price)
        );
        if (prices.size > 0) return prices;
      } catch (err) {
        logger.warn('Oracle API price fetch failed, falling back to CoinGecko', { err });
      }
    }

    // Fallback: CoinGecko public API
    try {
      const ids = SUPPORTED_ASSETS.map((a) => COINGECKO_IDS[a]).join(',');
      const response = await axios.get<Record<string, Record<string, number>>>(
        `https://api.coingecko.com/api/v3/simple/price?ids=${ids}&vs_currencies=usd`,
        { timeout: 8000 }
      );

      SUPPORTED_ASSETS.forEach((asset) => {
        const id = COINGECKO_IDS[asset];
        const price = response.data[id]?.usd;
        if (price !== undefined) {
          prices.set(asset, price);
        }
      });
    } catch (err) {
      logger.error('CoinGecko price fetch failed', { err });
    }

    return prices;
  }

  async pollAndBroadcast(): Promise<void> {
    const prices = await this.fetchPrices();
    const now = Math.floor(Date.now() / 1000);

    prices.forEach((price, asset) => {
      const last = this.lastPrices.get(asset);
      const changed = !last || last.price !== price;

      const update: PriceData = { asset, price, timestamp: now };
      this.lastPrices.set(asset, update);

      if (changed) {
        this.broadcastPriceUpdate(asset, update);
      }
    });
  }

  private broadcastPriceUpdate(asset: string, data: PriceData): void {
    const msg: ServerMessage = {
      type: 'price_update',
      asset: data.asset,
      price: data.price,
      timestamp: data.timestamp,
    };

    this.clientSubscriptions.forEach((subs, ws) => {
      if (subs.has(asset)) {
        this.send(ws, msg);
      }
    });
  }

  private startPricePolling(): void {
    this.pollAndBroadcast().catch((err) => logger.error('Initial price poll failed', { err }));

    this.pollIntervalId = setInterval(() => {
      this.pollAndBroadcast().catch((err) => logger.error('Price poll cycle failed', { err }));
    }, config.ws.priceUpdateIntervalMs);
  }

  private startHeartbeat(): void {
    this.heartbeatIntervalId = setInterval(() => {
      this.wss.clients.forEach((ws) => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.ping();
        } else {
          this.clientSubscriptions.delete(ws);
        }
      });
    }, config.ws.heartbeatIntervalMs);
  }

  close(): void {
    if (this.pollIntervalId) clearInterval(this.pollIntervalId);
    if (this.heartbeatIntervalId) clearInterval(this.heartbeatIntervalId);
    this.wss.close();
  }

  get clientCount(): number {
    return this.wss.clients.size;
  }

  get supportedAssets(): string[] {
    return [...SUPPORTED_ASSETS];
  }
}

export function createPriceWebSocket(server: Server): PriceWebSocketServer {
  return new PriceWebSocketServer(server);
}
