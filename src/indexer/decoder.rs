//! Decodes the well-known token events (ERC-20, ERC-721, ERC-1155) of a log into JSON.
//!
//! The decoded form holds every argument of the event, indexed or not:
//! `{"standard": "erc20", "event": "Transfer", "signature": "Transfer(address,address,uint256)",
//!   "args": {"from": "0x…", "to": "0x…", "value": "1500000000"}}`
//! Addresses are lowercase 0x-prefixed hex; numbers of up to 256 bits are decimal strings, since a
//! JSON number cannot hold them without losing precision.

use alloy::primitives::{Address, B256, U256};
use alloy::sol_types::SolEvent;
use serde_json::{json, Value};

mod erc20 {
    alloy::sol! {
        event Transfer(address indexed from, address indexed to, uint256 value);
        event Approval(address indexed owner, address indexed spender, uint256 value);
    }
}

mod erc721 {
    alloy::sol! {
        event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
        event Approval(address indexed owner, address indexed approved, uint256 indexed tokenId);
        event ApprovalForAll(address indexed owner, address indexed operator, bool approved);
    }
}

mod erc1155 {
    alloy::sol! {
        event TransferSingle(address indexed operator, address indexed from, address indexed to, uint256 id, uint256 value);
        event TransferBatch(address indexed operator, address indexed from, address indexed to, uint256[] ids, uint256[] values);
    }
}

fn address(a: &Address) -> String {
    format!("{:#x}", a)
}

fn number(n: &U256) -> String {
    n.to_string()
}

fn numbers(list: &[U256]) -> Vec<String> {
    list.iter().map(number).collect()
}

fn event(standard: &str, signature: &str, args: Value) -> Value {
    let name = signature.split('(').next().unwrap_or(signature);
    json!({ "standard": standard, "event": name, "signature": signature, "args": args })
}

/// Decodes a log into JSON, or returns None if it is not an event this decoder knows, or if its
/// topics and data do not have the shape that event requires (some tokens deviate from the standards)
pub fn decode_event(topics: &[B256], data: &[u8]) -> Option<Value> {
    let topic0 = *topics.first()?;
    // every supported event indexes its leading arguments as addresses (the first three for ERC-1155 transfers,
    // the first two otherwise); an indexed address is the address left-padded with zeros, and a topic that is
    // not would decode to an address that differs from the stored topic
    let address_topics = if topic0 == erc1155::TransferSingle::SIGNATURE_HASH || topic0 == erc1155::TransferBatch::SIGNATURE_HASH { 3 } else { 2 };
    if !topics.iter().skip(1).take(address_topics).all(|t| t.0[..12].iter().all(|b| *b == 0)) {
        return None;
    }
    let topics = topics.iter().copied();

    // ERC-20 and ERC-721 share the signatures of Transfer and Approval; the number of topics tells them apart:
    // ERC-20 indexes two arguments and carries the value in data, ERC-721 indexes the token id as well
    if topic0 == erc20::Transfer::SIGNATURE_HASH {
        return match topics.len() {
            3 => {
                let e = erc20::Transfer::decode_raw_log_validate(topics, data).ok()?;
                Some(event("erc20", erc20::Transfer::SIGNATURE, json!({ "from": address(&e.from), "to": address(&e.to), "value": number(&e.value) })))
            }
            4 => {
                let e = erc721::Transfer::decode_raw_log_validate(topics, data).ok()?;
                Some(event("erc721", erc721::Transfer::SIGNATURE, json!({ "from": address(&e.from), "to": address(&e.to), "tokenId": number(&e.tokenId) })))
            }
            _ => None,
        };
    }
    if topic0 == erc20::Approval::SIGNATURE_HASH {
        return match topics.len() {
            3 => {
                let e = erc20::Approval::decode_raw_log_validate(topics, data).ok()?;
                Some(event("erc20", erc20::Approval::SIGNATURE, json!({ "owner": address(&e.owner), "spender": address(&e.spender), "value": number(&e.value) })))
            }
            4 => {
                let e = erc721::Approval::decode_raw_log_validate(topics, data).ok()?;
                Some(event("erc721", erc721::Approval::SIGNATURE, json!({ "owner": address(&e.owner), "approved": address(&e.approved), "tokenId": number(&e.tokenId) })))
            }
            _ => None,
        };
    }
    // ERC-721 and ERC-1155 both declare ApprovalForAll with the same layout
    if topic0 == erc721::ApprovalForAll::SIGNATURE_HASH {
        let e = erc721::ApprovalForAll::decode_raw_log_validate(topics, data).ok()?;
        return Some(event("erc721/erc1155", erc721::ApprovalForAll::SIGNATURE,
            json!({ "owner": address(&e.owner), "operator": address(&e.operator), "approved": e.approved })));
    }
    if topic0 == erc1155::TransferSingle::SIGNATURE_HASH {
        let e = erc1155::TransferSingle::decode_raw_log_validate(topics, data).ok()?;
        return Some(event("erc1155", erc1155::TransferSingle::SIGNATURE, json!({
            "operator": address(&e.operator), "from": address(&e.from), "to": address(&e.to), "id": number(&e.id), "value": number(&e.value),
        })));
    }
    if topic0 == erc1155::TransferBatch::SIGNATURE_HASH {
        let e = erc1155::TransferBatch::decode_raw_log_validate(topics, data).ok()?;
        return Some(event("erc1155", erc1155::TransferBatch::SIGNATURE, json!({
            "operator": address(&e.operator), "from": address(&e.from), "to": address(&e.to), "ids": numbers(&e.ids), "values": numbers(&e.values),
        })));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::{Bytes, U256};
    use alloy::sol_types::SolValue;

    fn topic_of(a: Address) -> B256 {
        a.into_word()
    }

    fn word(n: u64) -> B256 {
        B256::from(U256::from(n))
    }

    const FROM: Address = Address::repeat_byte(0x11);
    const TO: Address = Address::repeat_byte(0x22);
    const OPERATOR: Address = Address::repeat_byte(0x33);

    #[test]
    fn test_erc20_transfer() {
        // 1,500 USDC: 6 decimals
        let topics = [erc20::Transfer::SIGNATURE_HASH, topic_of(FROM), topic_of(TO)];
        let data = U256::from(1_500_000_000u64).abi_encode();

        let decoded = decode_event(&topics, &data).unwrap();

        assert_eq!(decoded, json!({
            "standard": "erc20", "event": "Transfer", "signature": "Transfer(address,address,uint256)",
            "args": { "from": format!("0x{}", "11".repeat(20)), "to": format!("0x{}", "22".repeat(20)), "value": "1500000000" }
        }));
    }

    #[test]
    fn test_the_same_signature_with_four_topics_is_an_erc721_transfer() {
        let topics = [erc20::Transfer::SIGNATURE_HASH, topic_of(FROM), topic_of(TO), word(7777)];

        let decoded = decode_event(&topics, &[]).unwrap();

        assert_eq!(decoded["standard"], "erc721");
        assert_eq!(decoded["event"], "Transfer");
        assert_eq!(decoded["args"]["tokenId"], "7777");
        assert_eq!(decoded["args"]["to"], format!("0x{}", "22".repeat(20)));
    }

    #[test]
    fn test_numbers_beyond_64_bits_keep_every_digit() {
        let topics = [erc20::Transfer::SIGNATURE_HASH, topic_of(FROM), topic_of(TO)];
        let decoded = decode_event(&topics, &U256::MAX.abi_encode()).unwrap();
        assert_eq!(decoded["args"]["value"], U256::MAX.to_string());
    }

    #[test]
    fn test_erc20_and_erc721_approval() {
        let topics = [erc20::Approval::SIGNATURE_HASH, topic_of(FROM), topic_of(TO)];
        let decoded = decode_event(&topics, &U256::from(5u64).abi_encode()).unwrap();
        assert_eq!(decoded["standard"], "erc20");
        assert_eq!(decoded["args"], json!({ "owner": format!("0x{}", "11".repeat(20)), "spender": format!("0x{}", "22".repeat(20)), "value": "5" }));

        let topics = [erc20::Approval::SIGNATURE_HASH, topic_of(FROM), topic_of(TO), word(42)];
        let decoded = decode_event(&topics, &[]).unwrap();
        assert_eq!(decoded["standard"], "erc721");
        assert_eq!(decoded["args"]["approved"], format!("0x{}", "22".repeat(20)));
        assert_eq!(decoded["args"]["tokenId"], "42");
    }

    #[test]
    fn test_approval_for_all() {
        let topics = [erc721::ApprovalForAll::SIGNATURE_HASH, topic_of(FROM), topic_of(OPERATOR)];
        let decoded = decode_event(&topics, &true.abi_encode()).unwrap();
        assert_eq!(decoded["event"], "ApprovalForAll");
        assert_eq!(decoded["args"], json!({ "owner": format!("0x{}", "11".repeat(20)), "operator": format!("0x{}", "33".repeat(20)), "approved": true }));
    }

    #[test]
    fn test_erc1155_transfer_single() {
        let topics = [erc1155::TransferSingle::SIGNATURE_HASH, topic_of(OPERATOR), topic_of(FROM), topic_of(TO)];
        let data = (U256::from(9u64), U256::from(3u64)).abi_encode_params();

        let decoded = decode_event(&topics, &data).unwrap();

        assert_eq!(decoded["standard"], "erc1155");
        assert_eq!(decoded["args"]["operator"], format!("0x{}", "33".repeat(20)));
        assert_eq!(decoded["args"]["id"], "9");
        assert_eq!(decoded["args"]["value"], "3");
    }

    #[test]
    fn test_erc1155_transfer_batch_keeps_the_arrays() {
        let topics = [erc1155::TransferBatch::SIGNATURE_HASH, topic_of(OPERATOR), topic_of(FROM), topic_of(TO)];
        let data = (vec![U256::from(1u64), U256::from(2u64)], vec![U256::from(10u64), U256::from(20u64)]).abi_encode_params();

        let decoded = decode_event(&topics, &data).unwrap();

        assert_eq!(decoded["event"], "TransferBatch");
        assert_eq!(decoded["args"]["ids"], json!(["1", "2"]));
        assert_eq!(decoded["args"]["values"], json!(["10", "20"]));
    }

    #[test]
    fn test_unknown_events_are_not_decoded() {
        assert_eq!(decode_event(&[B256::repeat_byte(0x99)], &[1, 2, 3]), None);
        assert_eq!(decode_event(&[B256::repeat_byte(0x99), topic_of(FROM)], &[]), None);
    }

    #[test]
    fn test_a_log_without_topics_is_not_decoded() {
        assert_eq!(decode_event(&[], &Bytes::from_static(&[1, 2, 3])), None);
    }

    // tokens that deviate from the standard must fall back to the raw form instead of failing
    #[test]
    fn test_known_signature_with_the_wrong_shape_is_not_decoded() {
        // an ERC-20 Transfer whose data is too short
        let topics = [erc20::Transfer::SIGNATURE_HASH, topic_of(FROM), topic_of(TO)];
        assert_eq!(decode_event(&topics, &[0u8; 31]), None);
        // the value indexed as well, with nothing in data: 4 topics read as an ERC-721 transfer, which is the best that can be said
        // a Transfer with only one indexed address
        let topics = [erc20::Transfer::SIGNATURE_HASH, topic_of(FROM)];
        assert_eq!(decode_event(&topics, &U256::from(1u64).abi_encode()), None);
        // an address topic with dirty upper bytes is not a valid address
        let mut dirty = topic_of(FROM);
        dirty.0[0] = 0xff;
        let topics = [erc20::Transfer::SIGNATURE_HASH, dirty, topic_of(TO)];
        assert_eq!(decode_event(&topics, &U256::from(1u64).abi_encode()), None);
    }
}
