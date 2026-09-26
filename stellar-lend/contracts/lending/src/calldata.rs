//! # Calldata Compression for Complex Operations (issue #1045)
//!
//! Multi-step position management (e.g. "deposit A, deposit B, repay C,
//! withdraw D") normally needs one invocation per step, and every step carries
//! full-width arguments: a 32-byte contract `Address` per asset and a 16-byte
//! `i128` per amount, each wrapped in XDR `ScVal` framing. Transaction size is
//! a direct input to Soroban fees, so this repetition is expensive.
//!
//! This module defines a compact binary encoding that packs a sequence of
//! operations into a single `Bytes` argument:
//!
//! * assets are referenced by a **1-byte index** into an admin-managed asset
//!   dictionary instead of a full address;
//! * the opcode and a small asset index share a **single header byte**;
//! * amounts are **LEB128 varints**, so typical token amounts take 3–9 bytes
//!   instead of 16.
//!
//! A four-step sequence shrinks from ~4 × (32 + 16 + framing) bytes to roughly
//! 2 + 4 × (1 + ~6) bytes.
//!
//! ## Wire format (version 1)
//!
//! ```text
//! payload  := version:u8  count:u8  op{count}
//! op       := header:u8  [ext_index:u8]  amount:leb128
//! header   := opcode << 4 | short_index
//! ```
//!
//! * `version` must be [`CALLDATA_VERSION`].
//! * `count` is `1..=MAX_COMPRESSED_OPS`.
//! * `opcode` is one of [`OpCode`] (high nibble).
//! * `short_index` (low nibble) is the asset index when `< 0xF`. The value
//!   `0xF` is an escape: the full index follows in `ext_index` (`0..=255`).
//! * `amount` is an unsigned LEB128 integer in `1..=i128::MAX`, canonical
//!   (no redundant trailing `0x80`/`0x00` groups).
//!
//! Trailing bytes after the last op are rejected so a payload has exactly one
//! meaning.
//!
//! ## Execution
//!
//! [`execute`] authorises the user **once**, then runs the ops in order.
//! Consecutive `Deposit` ops are coalesced into a single
//! [`deposit_batch`](crate::deposit_batch) call, so they also share one write
//! of the packed deposit state (#1043/#1044). The whole payload is atomic: any
//! failing op reverts every op before it.

use crate::deposit_batch::{deposit_batch_with_auth, DepositRequest, MAX_BATCH_DEPOSITS};
use crate::pause::{is_paused, PauseType};
use soroban_sdk::{contracterror, contracttype, Address, Bytes, Env, Vec};

/// Current wire-format version.
pub const CALLDATA_VERSION: u8 = 1;
/// Maximum number of operations in one compressed payload.
pub const MAX_COMPRESSED_OPS: u32 = 32;
/// Maximum number of entries in the asset dictionary (1-byte index).
pub const MAX_DICTIONARY_ASSETS: u32 = 256;

/// Low-nibble value signalling that a full index byte follows the header.
const EXT_INDEX_ESCAPE: u8 = 0x0F;
/// Longest canonical LEB128 encoding of a value `<= i128::MAX` (127 bits / 7).
const MAX_VARINT_BYTES: u32 = 19;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CalldataError {
    /// Payload is shorter than its header or an op is truncated.
    Truncated = 1,
    /// Unknown wire-format version.
    UnsupportedVersion = 2,
    /// Op count is zero or above `MAX_COMPRESSED_OPS`.
    InvalidOpCount = 3,
    /// Header carries an opcode that is not defined.
    UnknownOpCode = 4,
    /// Amount is zero, overflows `i128`, or is not canonically encoded.
    InvalidAmount = 5,
    /// Extra bytes follow the last declared op.
    TrailingBytes = 6,
    /// Asset index is not present in the dictionary.
    UnknownAsset = 7,
    /// Dictionary is empty, too large, or contains duplicates.
    InvalidDictionary = 8,
    /// Caller is not the protocol admin.
    Unauthorized = 9,
    /// A deposit (or coalesced deposit batch) failed.
    DepositFailed = 10,
    /// A withdraw failed.
    WithdrawFailed = 11,
    /// A repay failed.
    RepayFailed = 12,
    /// A borrow-collateral deposit failed.
    CollateralFailed = 13,
    /// The targeted operation is paused.
    OperationPaused = 14,
}

/// Operations expressible in compressed calldata (high nibble of the header).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum OpCode {
    /// Vault deposit (`deposit`).
    Deposit = 1,
    /// Vault withdraw (`withdraw`).
    Withdraw = 2,
    /// Repay borrowed assets (`repay`).
    Repay = 3,
    /// Add collateral to a borrow position (`deposit_collateral`).
    DepositCollateral = 4,
}

impl OpCode {
    fn from_nibble(n: u8) -> Result<OpCode, CalldataError> {
        match n {
            1 => Ok(OpCode::Deposit),
            2 => Ok(OpCode::Withdraw),
            3 => Ok(OpCode::Repay),
            4 => Ok(OpCode::DepositCollateral),
            _ => Err(CalldataError::UnknownOpCode),
        }
    }

    fn nibble(self) -> u8 {
        self as u32 as u8
    }
}

/// One decoded operation, with its asset still expressed as a dictionary index.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompressedOp {
    pub op: OpCode,
    pub asset_index: u32,
    pub amount: i128,
}

/// Storage keys for the calldata asset dictionary.
#[contracttype]
#[derive(Clone)]
pub enum CalldataKey {
    /// `Vec<Address>`; position in the vector is the asset index.
    AssetDictionary,
}

// ── Decoding ─────────────────────────────────────────────────────────────

struct Cursor<'a> {
    bytes: &'a Bytes,
    pos: u32,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a Bytes) -> Self {
        Cursor { bytes, pos: 0 }
    }

    fn byte(&mut self) -> Result<u8, CalldataError> {
        let b = self.bytes.get(self.pos).ok_or(CalldataError::Truncated)?;
        self.pos += 1;
        Ok(b)
    }

    fn remaining(&self) -> u32 {
        self.bytes.len().saturating_sub(self.pos)
    }

    /// Read a canonical unsigned LEB128 value that fits in a positive `i128`.
    fn varint(&mut self) -> Result<i128, CalldataError> {
        let mut value: u128 = 0;
        let mut shift: u32 = 0;
        for i in 0..MAX_VARINT_BYTES {
            let byte = self.byte()?;
            let low = (byte & 0x7F) as u128;
            // Reject bits that would be shifted past bit 126 (i128::MAX).
            if shift > 0 && low > (i128::MAX as u128) >> shift {
                return Err(CalldataError::InvalidAmount);
            }
            value |= low << shift;
            if byte & 0x80 == 0 {
                // A zero final group after the first byte is a redundant,
                // non-canonical encoding.
                if i > 0 && byte == 0 {
                    return Err(CalldataError::InvalidAmount);
                }
                if value == 0 || value > i128::MAX as u128 {
                    return Err(CalldataError::InvalidAmount);
                }
                return Ok(value as i128);
            }
            shift += 7;
        }
        Err(CalldataError::InvalidAmount)
    }
}

/// Decode a compressed payload into its operations. Pure: no storage access.
pub fn decode(env: &Env, payload: &Bytes) -> Result<Vec<CompressedOp>, CalldataError> {
    let mut cur = Cursor::new(payload);

    if cur.byte()? != CALLDATA_VERSION {
        return Err(CalldataError::UnsupportedVersion);
    }
    let count = cur.byte()? as u32;
    if count == 0 || count > MAX_COMPRESSED_OPS {
        return Err(CalldataError::InvalidOpCount);
    }

    let mut ops = Vec::new(env);
    for _ in 0..count {
        let header = cur.byte()?;
        let op = OpCode::from_nibble(header >> 4)?;
        let short = header & 0x0F;
        let asset_index = if short == EXT_INDEX_ESCAPE {
            cur.byte()? as u32
        } else {
            short as u32
        };
        let amount = cur.varint()?;
        ops.push_back(CompressedOp {
            op,
            asset_index,
            amount,
        });
    }

    if cur.remaining() != 0 {
        return Err(CalldataError::TrailingBytes);
    }
    Ok(ops)
}

// ── Encoding ─────────────────────────────────────────────────────────────

fn push_varint(out: &mut Bytes, amount: i128) -> Result<(), CalldataError> {
    if amount <= 0 {
        return Err(CalldataError::InvalidAmount);
    }
    let mut v = amount as u128;
    loop {
        let low = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push_back(low);
            return Ok(());
        }
        out.push_back(low | 0x80);
    }
}

/// Encode operations into the version-1 wire format.
///
/// Exposed on-chain as a read-only helper so clients can build payloads via
/// simulation without re-implementing the codec.
pub fn encode(env: &Env, ops: &Vec<CompressedOp>) -> Result<Bytes, CalldataError> {
    let count = ops.len();
    if count == 0 || count > MAX_COMPRESSED_OPS {
        return Err(CalldataError::InvalidOpCount);
    }

    let mut out = Bytes::new(env);
    out.push_back(CALLDATA_VERSION);
    out.push_back(count as u8);

    for op in ops.iter() {
        if op.asset_index >= MAX_DICTIONARY_ASSETS {
            return Err(CalldataError::UnknownAsset);
        }
        let index = op.asset_index as u8;
        if index < EXT_INDEX_ESCAPE {
            out.push_back((op.op.nibble() << 4) | index);
        } else {
            out.push_back((op.op.nibble() << 4) | EXT_INDEX_ESCAPE);
            out.push_back(index);
        }
        push_varint(&mut out, op.amount)?;
    }
    Ok(out)
}

// ── Asset dictionary ─────────────────────────────────────────────────────

/// Replace the asset dictionary. Indices are positions in `assets`.
///
/// Changing the dictionary changes the meaning of existing payloads, so
/// clients should re-read it (via `get_calldata_assets`) before encoding.
pub fn set_dictionary(env: &Env, assets: &Vec<Address>) -> Result<(), CalldataError> {
    let len = assets.len();
    if len == 0 || len > MAX_DICTIONARY_ASSETS {
        return Err(CalldataError::InvalidDictionary);
    }
    for i in 0..len {
        let a = assets.get_unchecked(i);
        for j in (i + 1)..len {
            if assets.get_unchecked(j) == a {
                return Err(CalldataError::InvalidDictionary);
            }
        }
    }
    env.storage()
        .persistent()
        .set(&CalldataKey::AssetDictionary, assets);
    Ok(())
}

/// Current asset dictionary (empty if never configured).
pub fn get_dictionary(env: &Env) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&CalldataKey::AssetDictionary)
        .unwrap_or_else(|| Vec::new(env))
}

/// Resolve a dictionary index to its asset address.
pub fn resolve(dictionary: &Vec<Address>, index: u32) -> Result<Address, CalldataError> {
    dictionary.get(index).ok_or(CalldataError::UnknownAsset)
}

// ── Execution ────────────────────────────────────────────────────────────

fn flush_deposits(
    env: &Env,
    user: &Address,
    pending: &mut Vec<DepositRequest>,
) -> Result<(), CalldataError> {
    if pending.is_empty() {
        return Ok(());
    }
    deposit_batch_with_auth(env, user.clone(), pending.clone(), false)
        .map_err(|_| CalldataError::DepositFailed)?;
    *pending = Vec::new(env);
    Ok(())
}

/// Decode and execute a compressed payload on behalf of `user`.
///
/// Returns the number of operations executed.
pub fn execute(env: &Env, user: Address, payload: Bytes) -> Result<u32, CalldataError> {
    user.require_auth();

    let ops = decode(env, &payload)?;
    let dictionary = get_dictionary(env);

    let mut pending: Vec<DepositRequest> = Vec::new(env);
    for op in ops.iter() {
        let asset = resolve(&dictionary, op.asset_index)?;

        if op.op == OpCode::Deposit {
            pending.push_back(DepositRequest {
                asset,
                amount: op.amount,
            });
            if pending.len() == MAX_BATCH_DEPOSITS {
                flush_deposits(env, &user, &mut pending)?;
            }
            continue;
        }

        // Preserve ordering: earlier deposits land before any other op.
        flush_deposits(env, &user, &mut pending)?;

        match op.op {
            OpCode::Deposit => unreachable!(),
            OpCode::Withdraw => {
                crate::withdraw::withdraw_with_auth(env, user.clone(), asset, op.amount, false)
                    .map_err(|_| CalldataError::WithdrawFailed)?;
            }
            OpCode::Repay => {
                if is_paused(env, PauseType::Repay) {
                    return Err(CalldataError::OperationPaused);
                }
                crate::borrow::repay(env, user.clone(), asset, op.amount)
                    .map_err(|_| CalldataError::RepayFailed)?;
            }
            OpCode::DepositCollateral => {
                if is_paused(env, PauseType::Deposit) {
                    return Err(CalldataError::OperationPaused);
                }
                crate::borrow::deposit(env, user.clone(), asset, op.amount)
                    .map_err(|_| CalldataError::CollateralFailed)?;
            }
        }
    }
    flush_deposits(env, &user, &mut pending)?;

    Ok(ops.len())
}
