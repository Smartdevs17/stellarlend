/**
 * Elasticsearch sink for indexed events.
 *
 * Writes each ingested batch to the `stellarlend-events` write alias with the
 * bulk API. Documents use the RPC event id as `_id` and the `create` action,
 * so re-ingesting a ledger range after a restart is idempotent (409 conflicts
 * are expected and ignored). Index template, mappings and ILM policy live in
 * `backend/elasticsearch/`.
 */

import axios, { AxiosInstance } from 'axios';
import type { IndexedEvent } from './types';

export interface EventSink {
  readonly name: string;
  write(events: IndexedEvent[]): Promise<void>;
}

export interface ElasticsearchSinkOptions {
  url: string;
  index?: string;
  apiKey?: string;
  http?: AxiosInstance;
}

interface BulkItem {
  [action: string]: { status: number; error?: { type: string; reason: string } };
}

export function toElasticsearchDocument(e: IndexedEvent): Record<string, unknown> {
  return {
    '@timestamp': new Date(e.timestamp).toISOString(),
    event_id: e.id,
    type: e.type,
    envelope: e.envelope,
    contract: e.contract,
    ledger: e.ledger,
    tx_hash: e.txHash,
    accounts: e.accounts,
    actor: e.actor,
    counterparty: e.counterparty,
    module: e.module,
    action: e.action,
    asset: e.asset,
    // Keep full i128 precision as a keyword; the numeric copy is for aggregations.
    amount: e.amount,
    amount_numeric: e.amount !== undefined ? Number(e.amount) : undefined,
    schema_version: e.schemaVersion,
    schema_status: e.schemaStatus,
    topic: JSON.stringify(e.topic),
    data: e.data,
  };
}

export class ElasticsearchEventSink implements EventSink {
  readonly name = 'elasticsearch';
  private readonly http: AxiosInstance;
  private readonly index: string;

  constructor(options: ElasticsearchSinkOptions) {
    this.index = options.index ?? 'stellarlend-events';
    this.http =
      options.http ??
      axios.create({
        baseURL: options.url.replace(/\/$/, ''),
        timeout: 10_000,
        headers: options.apiKey ? { Authorization: `ApiKey ${options.apiKey}` } : {},
      });
  }

  async write(events: IndexedEvent[]): Promise<void> {
    if (events.length === 0) return;
    const body =
      events
        .flatMap((e) => [
          JSON.stringify({ create: { _index: this.index, _id: e.id } }),
          JSON.stringify(toElasticsearchDocument(e)),
        ])
        .join('\n') + '\n';

    const res = await this.http.post<{ errors: boolean; items: BulkItem[] }>('/_bulk', body, {
      headers: { 'Content-Type': 'application/x-ndjson' },
    });
    if (!res.data.errors) return;

    const failures = res.data.items
      .flatMap((item) => Object.values(item))
      .filter((r) => r.status >= 300 && r.status !== 409);
    if (failures.length) {
      const first = failures[0]?.error;
      throw new Error(
        `elasticsearch bulk rejected ${failures.length}/${events.length} events: ${first?.type ?? 'unknown'} ${first?.reason ?? ''}`.trim()
      );
    }
  }
}
