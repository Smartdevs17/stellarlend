import { createServer } from 'http';
import app from './app';
import { config } from './config';
import logger from './utils/logger';
import { createPriceWebSocket } from './ws/priceWebSocket';

const PORT = config.server.port;

const server = createServer(app);

// Attach WebSocket price server to the same HTTP server
createPriceWebSocket(server);

server.listen(PORT, () => {
  logger.info(`StellarLend API server running on port ${PORT}`);
  logger.info(`Environment: ${config.server.env}`);
  logger.info(`Network: ${config.stellar.network}`);
  logger.info(`WebSocket price feed: ws://localhost:${PORT}/api/ws/prices`);
});

process.on('unhandledRejection', (reason, promise) => {
  logger.error('Unhandled Rejection at:', promise, 'reason:', reason);
  process.exit(1);
});

process.on('uncaughtException', (error) => {
  logger.error('Uncaught Exception:', error);
  process.exit(1);
});
