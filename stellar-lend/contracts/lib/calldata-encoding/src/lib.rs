#![no_std]

extern crate alloc;

use soroban_sdk::{contracttype, Bytes, Env, Vec};

#[contracttype]
#[derive(Clone)]
pub struct EncodedOperation {
    pub op_type: u32,
    pub data: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub struct CompressedBatch {
    pub shared_params: Bytes,
    pub operations: Vec<EncodedOperation>,
    pub compression_ratio: u32,
}

pub struct CalldataEncoder;

impl CalldataEncoder {
    pub fn encode_varint(value: u64) -> alloc::vec::Vec<u8> {
        let mut result = alloc::vec::Vec::new();
        let mut v = value;

        while v >= 128 {
            let byte = ((v & 0x7F) as u8) | 0x80;
            result.push(byte);
            v >>= 7;
        }
        result.push((v & 0x7F) as u8);

        result
    }

    pub fn decode_varint(data: &[u8], offset: &mut usize) -> Result<u64, &'static str> {
        let mut result = 0u64;
        let mut shift = 0;

        loop {
            if *offset >= data.len() {
                return Err("Unexpected end of data");
            }

            let byte = data[*offset] as u64;
            *offset += 1;

            result |= (byte & 0x7F) << shift;

            if byte < 128 {
                break;
            }

            shift += 7;
            if shift >= 64 {
                return Err("Varint overflow");
            }
        }

        Ok(result)
    }

    pub fn encode_delta(current: i128, previous: i128) -> alloc::vec::Vec<u8> {
        let delta = current - previous;
        Self::encode_varint(delta as u64)
    }

    pub fn encode_bitmask(values: &[bool]) -> u8 {
        let mut mask = 0u8;
        for (i, &val) in values.iter().enumerate().take(8) {
            if val {
                mask |= 1 << i;
            }
        }
        mask
    }

    pub fn decode_bitmask(mask: u8) -> alloc::vec::Vec<bool> {
        let mut result = alloc::vec::Vec::new();
        for i in 0..8 {
            result.push((mask & (1 << i)) != 0);
        }
        result
    }

    pub fn compress_batch(
        _env: &Env,
        shared_asset: &[u8],
        shared_user: &[u8],
        operations: Vec<EncodedOperation>,
    ) -> CompressedBatch {
        let mut shared_params = alloc::vec::Vec::new();

        if !shared_asset.is_empty() {
            shared_params.extend_from_slice(shared_asset);
        }
        if !shared_user.is_empty() {
            shared_params.extend_from_slice(shared_user);
        }

        let n_ops = operations.len() as u32;
        let shared_len = shared_params.len() as u32;
        let original_size = n_ops * 256u32;
        let compressed_size = shared_len + (n_ops * 64u32);

        let compression_ratio = if compressed_size > 0 {
            ((original_size - compressed_size) * 100) / original_size
        } else {
            0
        };

        CompressedBatch {
            shared_params: Bytes::from_slice(_env, &shared_params),
            operations,
            compression_ratio,
        }
    }

    pub fn estimate_gas_savings(original_size: u32, compressed_size: u32) -> u32 {
        if original_size == 0 {
            return 0;
        }

        let reduction_percent = ((original_size - compressed_size) * 100) / original_size;
        (reduction_percent * 21000) / 100
    }
}

pub mod tests {
    use super::*;

    #[test]
    fn test_varint_encoding() {
        let encoded = CalldataEncoder::encode_varint(127);
        assert_eq!(encoded.len(), 1);

        let encoded = CalldataEncoder::encode_varint(128);
        assert_eq!(encoded.len(), 2);
    }

    #[test]
    fn test_bitmask_encoding() {
        let values = [true, false, true, false, false, false, false, false];
        let mask = CalldataEncoder::encode_bitmask(&values);
        let decoded = CalldataEncoder::decode_bitmask(mask);

        for (i, &expected) in values.iter().enumerate() {
            assert_eq!(decoded[i], expected);
        }
    }

    #[test]
    fn test_varint_roundtrip() {
        for value in [0u64, 1, 127, 128, 300, 65535, 1048576] {
            let encoded = CalldataEncoder::encode_varint(value);
            let mut offset = 0usize;
            let decoded = CalldataEncoder::decode_varint(&encoded, &mut offset).unwrap();
            assert_eq!(decoded, value);
            assert_eq!(offset, encoded.len());
        }
    }

    #[test]
    fn test_compress_batch_layout() {
        let env = soroban_sdk::Env::default();
        let op = EncodedOperation {
            op_type: 0,
            data: Bytes::from_slice(&env, &[0x00, 0x02, 0x05]),
        };
        let ops = soroban_sdk::vec![&env, op.clone()];
        let batch = CalldataEncoder::compress_batch(&env, &[0u8; 4], &[0u8; 4], ops);

        // Shared params carry the deduplicated asset/user bytes.
        assert_eq!(batch.shared_params.len(), 8);
        assert_eq!(batch.operations.len(), 1);
        assert_eq!(batch.operations.get(0).unwrap().op_type, 0);
        assert!(batch.compression_ratio > 0);
    }
}
