use alloy::consensus::{SignableTransaction, Transaction as _, TxEip1559, TxEnvelope, };
use alloy::consensus::transaction::SignerRecoverable;
use alloy::network::TransactionResponse;
use alloy::primitives::Signature;
use alloy::rpc::types::Transaction;
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;
use revm::context::TxEnv;
use revm::primitives::{U256};
use crate::error::AppErr;


pub fn alloy_tx_to_revm_tx(tx: &alloy::rpc::types::Transaction) -> Result<TxEnv, AppErr> {
    let tx_out = TxEnv::builder()
        .caller(tx.from())
        .nonce(tx.nonce())
        .kind(tx.kind())// this carries over .to()
        .value(tx.value())
        .data(tx.input().clone())
        .gas_limit(tx.gas_limit())
        .gas_price(TransactionResponse::max_fee_per_gas(tx).unwrap_or_default())
        .gas_priority_fee(tx.max_priority_fee_per_gas())
        .chain_id(tx.chain_id())
        .access_list(tx.access_list().cloned().unwrap_or_default())
        .blob_hashes(tx.blob_versioned_hashes().unwrap_or_default().to_vec())
        .max_fee_per_blob_gas(tx.max_fee_per_blob_gas().unwrap_or_default())
        .build()
        .map_err(|e| AppErr::TxTypeCastFailed(format!("{:?}", e)))?;

    Ok(tx_out)
}

pub fn validate_tx_signature(tx: &Transaction) -> Result<(), AppErr> {
    let curr_from = tx.from();
    let tx_env: TxEnvelope = match tx.clone().try_into() {
        Ok(tx_env) => tx_env,
        Err(e) => {
            eprintln!("failed");
            return Err(AppErr::TxValidationFailed(format!("{:?}", e)));
        },

    };

    let sig_hash = tx_env.signature_hash();
    let sig = tx_env.signature();

    let recovered_addr = sig.recover_address_from_prehash(&sig_hash);

    if recovered_addr.is_err() {
        return Err(AppErr::SigRecoveryFailed(format!("{:?}", tx_env)));
    }
    let recovered_addr = recovered_addr.unwrap();

    if recovered_addr != curr_from {
        return Err(AppErr::SigInvalid(String::from("recovered address differs from the initial from")))
    }
    Ok(())
}

pub fn validate_nonce(expected: u64, current: u64) -> Result<(), AppErr> {
    if current > expected {
        return Err(AppErr::NonceIsLower(expected.to_string()));
    } else if current < expected {
        return Err(AppErr::NonceIsHigher(expected.to_string()));
    }
    Ok(())
}

pub fn validate_balance(current_balance: U256, value: U256, gas_limit: U256, max_fee_per_gas: U256) -> Result<(), AppErr> {
    let required = value + (gas_limit & max_fee_per_gas);
    if current_balance < required {
        return Err(AppErr::InsufficientBalance(format!("{current} is lower than {required}", current=current_balance, required=required)));
    }
    Ok(())
}


pub fn eip1559_to_alloy_tx(tx: TxEip1559, signer: &PrivateKeySigner) -> Result<Transaction, AppErr> {
    let sig_hash = tx.signature_hash();
    let sig: Signature = match signer.sign_hash_sync(&sig_hash) {
        Ok(sig) => sig,
        Err(e) => {
            return Err(AppErr::SigInvalid(format!("failed to sign transaction: {e}")));
        }
    };
    let env = TxEnvelope::Eip1559(tx.into_signed(sig));
    let recovered = env.clone().try_into_recovered().unwrap();

    let out_tx = Transaction{
        inner: recovered,
        block_hash: None,
        block_number: None,
        transaction_index: None,
        effective_gas_price: None,
        block_timestamp: None,
    };

    Ok(out_tx)
}

// address: 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
// PvK: 0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80
//
#[cfg(test)]
mod tests {
    use alloy::consensus::{TxEip1559};
    use alloy::signers::local::PrivateKeySigner;
    use revm::primitives::{address, bytes};
    use revm::primitives::alloy_primitives::utils::ParseUnits;
    use crate::common::helpers::{eip1559_to_alloy_tx, validate_tx_signature};

    #[test]
    fn test_validate_tx_signature() {
        let tx = TxEip1559 {
            chain_id: 1,
            nonce: 0,
            max_priority_fee_per_gas: 1_000_000_000,
            max_fee_per_gas: 20_000_000_000,
            gas_limit: 21_000,
            to: address!("70997970C51812dc3A010C7d01b50e0d17dc79C8").into(),
            value: ParseUnits::from(1_000_000_000_000_000_000u64).try_into().unwrap(),
            input: bytes!(""),
            access_list: Default::default(),
        };
        let ps = PrivateKeySigner::random();
        let rpc_tx = eip1559_to_alloy_tx(tx, &ps);

        assert_eq!(false, rpc_tx.is_err());

        if rpc_tx.is_ok() {
            let result = validate_tx_signature(&rpc_tx.unwrap());
            assert!(result.is_ok());
            assert_eq!(None, result.err())
        }
    }

}
