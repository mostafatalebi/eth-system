use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::str::FromStr;
use alloy::consensus::{BlockHeader, Transaction as ConsensusTransaction};
use alloy::network::TransactionResponse;
use alloy::primitives::{keccak256, Address, U256};
use alloy::rlp::BytesMut;
use alloy::rpc::types::{Block, BlockTransactions};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use tokio_postgres::Row;
use tokio_postgres::types::{to_sql_checked, FromSql, IsNull, Kind, ToSql, Type};
use crate::error::CoreError;



#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AccountModel {
    pub address: Vec<u8>,
    pub nonce: i64,
    /// aka balance
    pub value: String,
    pub code_hash: Vec<u8>,
    pub account_type: AccountType,
    /// None when the account is not an EIP-7702 delegated EOA
    pub delegate_address: Option<Vec<u8>>,
    /// block at which the delegation last changed; None until delegation tracking sets it
    pub delegate_updated_at_block_number: Option<i64>,
    pub first_block_number: i64,
    pub last_block_number: i64,
    pub create_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// EIP-7702 delegation designator prefix; a delegated EOA's code is `0xef0100 || delegate (20 bytes)`
const DELEGATION_PREFIX: [u8; 3] = [0xef, 0x01, 0x00];
const DELEGATION_CODE_LEN: usize = 23;

impl AccountModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// builds an AccountModel from a row of the accounts table. `balance` is numeric in the
    /// database, so select the columns explicitly with `balance::text as balance` (a String
    /// cannot be read from numeric). Panics (like Row::get) on a missing column or a type mismatch
    pub fn new_from_db_row(row: &Row) -> AccountModel {
        return Self {
            address: row.get("address"),
            nonce: row.get("nonce"),
            value: row.get("balance"),
            code_hash: row.get("code_hash"),
            account_type: row.get("account_type"),
            delegate_address: row.get("delegate_address"),
            delegate_updated_at_block_number: row.get("delegate_updated_at_block_number"),
            first_block_number: row.get("first_block_number"),
            last_block_number: row.get("last_block_number"),
            create_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }

    /// builds an AccountModel from the account's state at `block_number`, as returned by
    /// eth_getBalance, eth_getTransactionCount and eth_getCode (all queried at that block).
    /// Account state is not part of a block, so it cannot be built from the block alone;
    /// use `addresses_from_rpc_block` to find which accounts a block touched.
    ///
    /// EIP-7702 delegated EOAs are classified as EOA (account type is immutable) and get
    /// their delegate recorded in `delegate_address`
    pub fn new_from_rpc_state(address: Address, block_number: u64, balance: U256, nonce: u64, code: &[u8]) -> Result<AccountModel, CoreError> {
        let block_number = i64::try_from(block_number)
            .map_err(|e| CoreError::BadArgument(format!("invalid block number {block_number}: {e}")))?;
        let nonce = i64::try_from(nonce)
            .map_err(|e| CoreError::BadArgument(format!("invalid nonce {nonce}: {e}")))?;

        let delegate = Self::delegate_of_code(code);
        let account_type = if code.is_empty() || delegate.is_some() {
            AccountType::EOA
        } else {
            AccountType::Contract
        };

        Ok(Self {
            address: address.to_vec(),
            nonce,
            value: balance.to_string(),
            code_hash: keccak256(code).to_vec(),
            account_type,
            delegate_address: delegate.map(|d| d.to_vec()),
            delegate_updated_at_block_number: None,
            first_block_number: block_number,
            last_block_number: block_number,
            create_at: Default::default(),
            updated_at: Default::default(),
        })
    }

    /// returns the delegate address if `code` is an EIP-7702 delegation designator
    pub fn delegate_of_code(code: &[u8]) -> Option<Address> {
        if code.len() == DELEGATION_CODE_LEN && code.starts_with(&DELEGATION_PREFIX) {
            return Some(Address::from_slice(&code[DELEGATION_PREFIX.len()..]));
        }
        None
    }

    /// addresses whose state the block may have changed, in first-seen order, without duplicates:
    /// fee recipient, tx senders and recipients, contracts created by top-level CREATE txs,
    /// EIP-7702 authorities and withdrawal recipients.
    /// Addresses touched only by internal calls (CALL/CREATE/CREATE2 inside contracts) are
    /// not visible in a block and need traces (debug_traceBlock / trace_block)
    pub fn addresses_from_rpc_block(block: &Block) -> Vec<Address> {
        let mut addresses = Vec::new();
        let mut seen = HashSet::new();
        let mut push = |a: Address| {
            if seen.insert(a) {
                addresses.push(a);
            }
        };

        push(block.header.beneficiary());

        if let BlockTransactions::Full(txs) = &block.transactions {
            for tx in txs {
                let from = TransactionResponse::from(tx);
                push(from);
                match ConsensusTransaction::to(tx) {
                    Some(to) => push(to),
                    None => push(from.create(ConsensusTransaction::nonce(tx))),
                }
                // invalid authorizations are skipped by the protocol; an authority listed
                // here is only a candidate, so its state must still be read from the node
                for auth in ConsensusTransaction::authorization_list(tx).unwrap_or_default() {
                    if let Ok(authority) = auth.recover_authority() {
                        push(authority);
                    }
                }
            }
        }

        if let Some(withdrawals) = &block.withdrawals {
            for w in withdrawals.iter() {
                push(w.address);
            }
        }

        addresses
    }

    /// values in the same order as the columns of the accounts insert query:
    /// (address, account_type, nonce, balance, code_hash, first_block_number, last_block_number,
    ///  delegate_address, delegate_updated_at_block_number); bind balance as `$4::text::numeric`
    pub fn get_prepared_values(&self) -> Vec<&(dyn ToSql + Sync)> {
        let mut values: Vec<&(dyn ToSql+Sync)> = vec![];
        values.push(&self.address);
        values.push(&self.account_type);
        values.push(&self.nonce);
        values.push(&self.value);
        values.push(&self.code_hash);
        values.push(&self.first_block_number);
        values.push(&self.last_block_number);
        values.push(&self.delegate_address);
        values.push(&self.delegate_updated_at_block_number);
        values
    }
}

/// stored in the database as "eoa" or "contract", either in the `account_type`
/// postgres enum or in a text/varchar column
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccountType {
    #[default]
    EOA,
    Contract
}

impl AccountType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountType::EOA => "eoa",
            AccountType::Contract => "contract",
        }
    }
}

impl fmt::Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AccountType {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, CoreError> {
        match s {
            "eoa" => Ok(AccountType::EOA),
            "contract" => Ok(AccountType::Contract),
            _ => Err(CoreError::BadArgument(format!("unknown account type: {s}"))),
        }
    }
}

/// name of the postgres enum type that backs AccountType (see migrations/V4__create_table_accounts.sql)
const PG_ENUM_NAME: &str = "account_type";

/// accepts the `account_type` postgres enum (its labels must cover every variant),
/// plus text/varchar columns. Enum values travel as their label text, so both use
/// the same text encoding
fn accepts_pg_type(ty: &Type) -> bool {
    if let Kind::Enum(labels) = ty.kind() {
        return ty.name() == PG_ENUM_NAME
            && [AccountType::EOA, AccountType::Contract].iter()
                .all(|t| labels.iter().any(|l| l == t.as_str()));
    }
    <&str as ToSql>::accepts(ty)
}

impl ToSql for AccountType {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        self.as_str().to_sql(ty, out)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_pg_type(ty)
    }

    to_sql_checked!();
}

impl<'a> FromSql<'a> for AccountType {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let s = <&str as FromSql>::from_sql(ty, raw)?;
        Ok(s.parse::<AccountType>()?)
    }

    fn accepts(ty: &Type) -> bool {
        accepts_pg_type(ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_type_string_round_trip() {
        for t in [AccountType::EOA, AccountType::Contract] {
            assert_eq!(t.to_string().parse::<AccountType>().unwrap(), t);
        }
        assert!("unknown".parse::<AccountType>().is_err());
    }

    #[test]
    fn new_from_rpc_state_classifies_accounts() {
        let addr = Address::repeat_byte(0x11);
        let delegate = Address::repeat_byte(0x22);

        let eoa = AccountModel::new_from_rpc_state(addr, 10, U256::from(5u64), 3, &[]).unwrap();
        assert_eq!(eoa.account_type, AccountType::EOA);
        assert_eq!(eoa.delegate_address, None);
        assert_eq!(eoa.code_hash, alloy::primitives::KECCAK256_EMPTY.to_vec());
        assert_eq!(eoa.value, "5");
        assert_eq!((eoa.first_block_number, eoa.last_block_number), (10, 10));

        let mut designator = DELEGATION_PREFIX.to_vec();
        designator.extend_from_slice(delegate.as_slice());
        let delegated = AccountModel::new_from_rpc_state(addr, 10, U256::ZERO, 4, &designator).unwrap();
        assert_eq!(delegated.account_type, AccountType::EOA);
        assert_eq!(delegated.delegate_address, Some(delegate.to_vec()));

        let contract = AccountModel::new_from_rpc_state(addr, 10, U256::ZERO, 1, &[0x60, 0x80, 0x60, 0x40]).unwrap();
        assert_eq!(contract.account_type, AccountType::Contract);
        assert_eq!(contract.delegate_address, None);

        assert!(AccountModel::new_from_rpc_state(addr, u64::MAX, U256::ZERO, 0, &[]).is_err());
    }

    #[test]
    fn prepared_values_match_insert_column_count() {
        assert_eq!(AccountModel::new().get_prepared_values().len(), 9);
    }

    fn pg_enum(name: &str, labels: &[&str]) -> Type {
        Type::new(name.to_string(), 0, Kind::Enum(labels.iter().map(|l| l.to_string()).collect()), "public".to_string())
    }

    #[test]
    fn account_type_accepts_pg_types() {
        assert!(<AccountType as ToSql>::accepts(&Type::TEXT));
        assert!(<AccountType as FromSql>::accepts(&Type::VARCHAR));
        assert!(<AccountType as ToSql>::accepts(&pg_enum("account_type", &["eoa", "contract"])));
        assert!(<AccountType as FromSql>::accepts(&pg_enum("account_type", &["eoa", "contract"])));
        // a different enum, or one missing a variant, is rejected
        assert!(!<AccountType as ToSql>::accepts(&pg_enum("tx_status", &["eoa", "contract"])));
        assert!(!<AccountType as FromSql>::accepts(&pg_enum("account_type", &["eoa"])));
        assert!(!<AccountType as ToSql>::accepts(&Type::INT8));
    }

    #[test]
    fn account_type_enum_wire_round_trip() {
        let ty = pg_enum("account_type", &["eoa", "contract"]);
        for t in [AccountType::EOA, AccountType::Contract] {
            let mut buf = BytesMut::new();
            t.to_sql_checked(&ty, &mut buf).unwrap();
            assert_eq!(&buf[..], t.as_str().as_bytes());
            assert_eq!(AccountType::from_sql(&ty, &buf).unwrap(), t);
        }
    }
}

