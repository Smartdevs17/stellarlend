/**
 * Issue #1373 – OpenAPI specification generated from the query route
 * definitions. `ROUTES` is the single source of truth used by both the HTTP
 * server and `scripts/generate-openapi.ts`, so the published spec cannot
 * drift from the implementation.
 */

export interface QueryRoute {
  method: 'get';
  path: string;
  summary: string;
  description?: string;
  parameters?: Array<{
    name: string;
    in: 'path' | 'query';
    required?: boolean;
    description?: string;
    schema: Record<string, unknown>;
  }>;
  response: Record<string, unknown>;
}

export const ROUTES: QueryRoute[] = [
  {
    method: 'get',
    path: '/prices',
    summary: 'Get the latest aggregated price for every tracked asset',
    parameters: [
      {
        name: 'asset',
        in: 'query',
        required: false,
        description: 'Optional comma-separated asset filter (e.g. XLM,USDC)',
        schema: { type: 'string', example: 'XLM,USDC' },
      },
    ],
    response: {
      type: 'object',
      properties: {
        success: { type: 'boolean', example: true },
        mode: { type: 'string', enum: ['push', 'pull', 'both'], example: 'pull' },
        data: {
          type: 'array',
          items: { $ref: '#/components/schemas/Price' },
        },
        timestamp: { type: 'string', format: 'date-time' },
      },
    },
  },
  {
    method: 'get',
    path: '/prices/{asset}',
    summary: 'Get the latest aggregated price for one asset',
    parameters: [
      {
        name: 'asset',
        in: 'path',
        required: true,
        description: 'Asset symbol (XLM, USDC, BTC, …)',
        schema: { type: 'string', example: 'XLM' },
      },
    ],
    response: {
      oneOf: [{ $ref: '#/components/schemas/Price' }, { $ref: '#/components/schemas/Error' }],
    },
  },
  {
    method: 'get',
    path: '/prices/{asset}/history',
    summary: 'Query historical prices with filters and cursor pagination',
    description:
      'Results are served from a short-lived cache and invalidated when new ' +
      'prices arrive. `limit` is capped at 100 server-side.',
    parameters: [
      {
        name: 'asset',
        in: 'path',
        required: true,
        description: 'Asset symbol',
        schema: { type: 'string', example: 'XLM' },
      },
      {
        name: 'from',
        in: 'query',
        required: false,
        description: 'Inclusive lower bound (unix seconds)',
        schema: { type: 'integer', example: 1735689600 },
      },
      {
        name: 'to',
        in: 'query',
        required: false,
        description: 'Inclusive upper bound (unix seconds)',
        schema: { type: 'integer', example: 1735776000 },
      },
      {
        name: 'limit',
        in: 'query',
        required: false,
        description: 'Page size (default 20, maximum 100)',
        schema: { type: 'integer', minimum: 1, maximum: 100, default: 20 },
      },
      {
        name: 'cursor',
        in: 'query',
        required: false,
        description: "Previous page's `pagination.nextCursor` value",
        schema: { type: 'string', example: '1735689600' },
      },
    ],
    response: {
      type: 'object',
      properties: {
        success: { type: 'boolean', example: true },
        data: {
          type: 'object',
          properties: {
            asset: { type: 'string', example: 'XLM' },
            entries: {
              type: 'array',
              items: { $ref: '#/components/schemas/HistoryEntry' },
            },
            cached: { type: 'boolean', example: false },
            pagination: { $ref: '#/components/schemas/Pagination' },
          },
        },
        timestamp: { type: 'string', format: 'date-time' },
      },
    },
  },
];

/**
 * Build the OpenAPI 3.0 document. Pure function of `ROUTES`.
 */
export function buildOpenApiDocument(options: { version?: string } = {}) {
  return {
    openapi: '3.0.3',
    info: {
      title: 'StellarLend Oracle Query API',
      description:
        'Read-side API for the StellarLend price oracle (pull model) with ' + 'historical queries.',
      version: options.version ?? '1.0.0',
    },
    servers: [{ url: 'http://localhost:3002', description: 'Local query server' }],
    paths: Object.fromEntries(
      ROUTES.map((route) => [
        route.path,
        {
          [route.method]: {
            summary: route.summary,
            ...(route.description ? { description: route.description } : {}),
            parameters: route.parameters ?? [],
            responses: {
              200: {
                description: 'Successful response',
                content: { 'application/json': { schema: route.response } },
              },
              400: { $ref: '#/components/responses/BadRequest' },
              404: { $ref: '#/components/responses/NotFound' },
            },
          },
        },
      ])
    ),
    components: {
      schemas: {
        Price: {
          type: 'object',
          properties: {
            asset: { type: 'string', example: 'XLM' },
            price: { type: 'string', example: '0.1100000' },
            timestamp: { type: 'integer', example: 1735689600 },
            confidence: { type: 'number', example: 0.98 },
            sources: {
              type: 'array',
              items: { type: 'string', example: 'coingecko' },
            },
          },
        },
        HistoryEntry: {
          type: 'object',
          properties: {
            price: { type: 'string', example: '0.1100000' },
            timestamp: { type: 'integer', example: 1735689600 },
          },
        },
        Pagination: {
          type: 'object',
          properties: {
            limit: { type: 'integer', example: 20 },
            count: { type: 'integer', example: 20 },
            total: { type: 'integer', example: 100 },
            hasMore: { type: 'boolean', example: true },
            nextCursor: { type: 'string', nullable: true, example: '1735689600' },
          },
        },
        Error: {
          type: 'object',
          properties: {
            success: { type: 'boolean', example: false },
            error: { type: 'string', example: 'Asset not found' },
          },
        },
      },
      responses: {
        BadRequest: {
          description: 'Invalid query parameters',
          content: {
            'application/json': { schema: { $ref: '#/components/schemas/Error' } },
          },
        },
        NotFound: {
          description: 'Unknown route or asset',
          content: {
            'application/json': { schema: { $ref: '#/components/schemas/Error' } },
          },
        },
      },
    },
  };
}
