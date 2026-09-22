pub mod mocks;

use std::error::Error;
use std::str::FromStr;
use alloy::rlp::BytesMut;
use bigdecimal::BigDecimal;
use bigdecimal::num_traits::FromBytes;
use num_bigint::{BigInt, BigUint};
use tokio_postgres::error::DbError;
use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};
use crate::error::CoreError;

#[derive(Default, Debug)]
pub struct SqlNumeric {
    v_big_uint: Option<BigUint>,
    v_big_int: Option<BigInt>,
    v_big_decimal: Option<BigDecimal>,
}

impl SqlNumeric {
    pub fn new_big_uint(b: &'_ [u8]) -> Self {
        Self{v_big_uint: Some(BigUint::from_bytes_be(b)), v_big_int: None, v_big_decimal: None }
    }

    pub fn big_uint(&self) -> Option<BigUint> {
        self.v_big_uint.clone()
    }

    pub fn big_int(&self) -> Option<BigInt> {
        self.v_big_int.clone()
    }

    pub fn big_decimal(&self) -> Option<BigDecimal> {
        self.v_big_decimal.clone()
    }
}

impl FromSql<'_> for SqlNumeric  {
    fn from_sql<'a>(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        match ty {
            &Type::NUMERIC => {
                return Ok(SqlNumeric::new_big_uint(raw));
            },
            _ => {
                return Err(Box::from(CoreError::DbTypeDecodingFailed(format!("{:?} is not a numeric type", ty))));
            }
        }
    }

    fn accepts(ty: &Type) -> bool {
        matches!(ty, &Type::NUMERIC)
    }
}

pub type SqlB32 = [u8; 32];