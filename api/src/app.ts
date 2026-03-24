import express, { Application } from 'express';
import helmet from 'helmet';
import cors from 'cors';
import rateLimit from 'express-rate-limit';
import { config } from './config';
import lendingRoutes from './routes/lending.routes';
import healthRoutes from './routes/health.routes';
import { errorHandler } from './middleware/errorHandler';
import logger from './utils/logger';

const app: Application = express();

app.use(
  helmet({
    hsts: {
      maxAge: 31536000,
      includeSubDomains: true,
      preload: true,
    },
  })
);

// Enforce HTTPS in production
if (config.server.env === 'production') {
  app.use((req, res, next) => {
    if (req.header('x-forwarded-proto') !== 'https' && !req.secure) {
      return res.redirect(`https://${req.header('host')}${req.url}`);
    }
    next();
  });
}

app.use(cors());
app.use(express.json());
app.use(express.urlencoded({ extended: true }));

const limiter = rateLimit({
  windowMs: config.rateLimit.windowMs,
  max: config.rateLimit.maxRequests,
  message: 'Too many requests from this IP, please try again later.',
});

app.use('/api/', limiter);

app.use('/api/health', healthRoutes);
app.use('/api/lending', lendingRoutes);

app.use(errorHandler);

export default app;
