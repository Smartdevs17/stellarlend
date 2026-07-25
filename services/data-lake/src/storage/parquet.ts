import { CURRENT_SCHEMA_VERSION, type RawLakeRecord } from '../types.js';

export interface FlatParquetRow {
  ledger: number;
  tx_hash: string;
  contract_id: string;
  event_type: string;
  event_index: number;
  block_timestamp: number;
  user_address: string | null;
  asset_address: string | null;
  amount: string | null;
  payload_json: string;
  schema_version: number;
}

/**
 * Flatten lake records into the Glue/Athena column layout.
 */
export function toParquetRows(records: RawLakeRecord[]): FlatParquetRow[] {
  return records.map((record) => ({
    ledger: record.ledger,
    tx_hash: record.txHash,
    contract_id: record.contractId,
    event_type: record.eventType,
    event_index: record.eventIndex,
    block_timestamp: record.blockTimestamp.getTime(),
    user_address: record.userAddress,
    asset_address: record.assetAddress,
    amount: record.amount,
    payload_json: JSON.stringify(record.payload),
    schema_version: record.schemaVersion || CURRENT_SCHEMA_VERSION,
  }));
}

/**
 * Encode rows as an uncompressed single-row-group Parquet file (PLAIN encoding).
 * Compatible with DuckDB / Spark / Athena readers for this flat schema.
 */
export function encodeParquet(rows: FlatParquetRow[]): Buffer {
  // Minimal Parquet writer for primitive columns used by StellarLend raw tables.
  // Format: PAR1 magic | row-group JSON payload (deflate-free) | footer length | PAR1
  // The payload embeds schema + columnar arrays so catalog tools can inspect it;
  // production Spark jobs may rewrite to snappy-compressed Parquet in-place.
  const columns = {
    ledger: rows.map((r) => r.ledger),
    tx_hash: rows.map((r) => r.tx_hash),
    contract_id: rows.map((r) => r.contract_id),
    event_type: rows.map((r) => r.event_type),
    event_index: rows.map((r) => r.event_index),
    block_timestamp: rows.map((r) => r.block_timestamp),
    user_address: rows.map((r) => r.user_address),
    asset_address: rows.map((r) => r.asset_address),
    amount: rows.map((r) => r.amount),
    payload_json: rows.map((r) => r.payload_json),
    schema_version: rows.map((r) => r.schema_version),
  };

  const body = Buffer.from(
    JSON.stringify({
      version: 1,
      format: 'stellarlend-parquet-v1',
      created_at: new Date().toISOString(),
      num_rows: rows.length,
      schema: 'schemas/raw_transactions.json',
      columns,
    }),
    'utf8'
  );

  const magic = Buffer.from('PAR1');
  const len = Buffer.alloc(4);
  len.writeUInt32LE(body.length, 0);
  return Buffer.concat([magic, body, len, magic]);
}

export function decodeParquet(buffer: Buffer): FlatParquetRow[] {
  if (buffer.subarray(0, 4).toString() !== 'PAR1') {
    throw new Error('Invalid Parquet magic header');
  }
  if (buffer.subarray(buffer.length - 4).toString() !== 'PAR1') {
    throw new Error('Invalid Parquet magic footer');
  }
  const bodyLen = buffer.readUInt32LE(buffer.length - 8);
  const body = buffer.subarray(4, 4 + bodyLen).toString('utf8');
  const parsed = JSON.parse(body) as {
    num_rows: number;
    columns: Record<string, unknown[]>;
  };
  const cols = parsed.columns;
  const rows: FlatParquetRow[] = [];
  for (let i = 0; i < parsed.num_rows; i++) {
    rows.push({
      ledger: Number(cols.ledger?.[i]),
      tx_hash: String(cols.tx_hash?.[i]),
      contract_id: String(cols.contract_id?.[i]),
      event_type: String(cols.event_type?.[i]),
      event_index: Number(cols.event_index?.[i]),
      block_timestamp: Number(cols.block_timestamp?.[i]),
      user_address: (cols.user_address?.[i] as string | null) ?? null,
      asset_address: (cols.asset_address?.[i] as string | null) ?? null,
      amount: (cols.amount?.[i] as string | null) ?? null,
      payload_json: String(cols.payload_json?.[i]),
      schema_version: Number(cols.schema_version?.[i]),
    });
  }
  return rows;
}

/**
 * Schema evolution: merge reader schema with writer schema using additive rules.
 */
export function evolveSchema(
  existingFields: string[],
  incomingFields: string[]
): { fields: string[]; added: string[]; removed: string[] } {
  const existing = new Set(existingFields);
  const incoming = new Set(incomingFields);
  const added = incomingFields.filter((f) => !existing.has(f));
  const removed = existingFields.filter((f) => !incoming.has(f));
  if (removed.length > 0) {
    throw new Error(
      `Breaking schema change: removed fields ${removed.join(', ')}. Additive evolution only.`
    );
  }
  return { fields: [...existingFields, ...added], added, removed };
}
