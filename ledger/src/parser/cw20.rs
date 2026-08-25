use crate::amount::{parse_raw_amount, Amount};
use crate::error::LedgerResult;

/// One wasm event attribute. Keys are as emitted (not lowercased).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cw20Attribute {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cw20Event {
    pub event_type: String,
    pub attributes: Vec<Cw20Attribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cw20Transfer {
    pub contract: String,
    pub from: String,
    pub to: String,
    pub amount: Amount,
    pub action: String,
}

fn wasm_attr_last<'a>(attrs: &'a [Cw20Attribute], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .rev()
        .find(|a| a.key == key)
        .map(|a| a.value.as_str())
}

/// GitLab cl8y-dex-terraclassic#285: only `_contract_address` is wasmd-stamped.
/// A forgeable `contract_address` attribute must never be treated as the emitter.
fn wasm_contract_addr<'a>(attrs: &'a [Cw20Attribute]) -> Option<&'a str> {
    wasm_attr_last(attrs, "_contract_address")
}

fn is_wasm_lifecycle(event_type: &str) -> bool {
    event_type == "wasm" || event_type == "wasm-wasm"
}

fn is_balance_action(action: &str) -> bool {
    matches!(action, "transfer" | "send" | "mint" | "burn")
}

/// Parse CL8Y CW20 transfer-like events. Unrelated contracts are ignored.
/// `registered` is the lowercase registered Terra set used only for filtering
/// which legs to keep; the caller still decides persistence.
pub fn parse_cw20_wasm_events(
    events: &[Cw20Event],
    cl8y_token: &str,
    registered: &[&str],
) -> LedgerResult<Vec<Cw20Transfer>> {
    let token = cl8y_token.trim().to_ascii_lowercase();
    let registered_l: Vec<String> = registered
        .iter()
        .map(|a| a.trim().to_ascii_lowercase())
        .collect();
    let mut out = Vec::new();

    for event in events {
        if !is_wasm_lifecycle(&event.event_type) {
            continue;
        }
        let Some(contract) = wasm_contract_addr(&event.attributes) else {
            continue;
        };
        if contract.to_ascii_lowercase() != token {
            continue;
        }
        let Some(action) = wasm_attr_last(&event.attributes, "action") else {
            continue;
        };
        if !is_balance_action(action) {
            continue;
        }
        let from = wasm_attr_last(&event.attributes, "from")
            .or_else(|| wasm_attr_last(&event.attributes, "sender"))
            .unwrap_or("")
            .to_ascii_lowercase();
        let to = wasm_attr_last(&event.attributes, "to")
            .or_else(|| wasm_attr_last(&event.attributes, "recipient"))
            .unwrap_or("")
            .to_ascii_lowercase();
        let Some(amount_raw) = wasm_attr_last(&event.attributes, "amount") else {
            continue;
        };
        let amount = parse_raw_amount(amount_raw)?;
        let touches = registered_l.iter().any(|w| w == &from || w == &to);
        if !touches {
            continue;
        }
        out.push(Cw20Transfer {
            contract: contract.to_ascii_lowercase(),
            from,
            to,
            amount,
            action: action.to_string(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amount::human_to_raw;

    const CL8Y: &str = "terra16wtml2q66g82fdkx66tap0qjkahqwp4lwq3ngtygacg5q0kzycgqvhpax3";
    const ALICE: &str = "terra1alice00000000000000000000000000000000";

    fn attr(k: &str, v: &str) -> Cw20Attribute {
        Cw20Attribute {
            key: k.into(),
            value: v.into(),
        }
    }

    fn wasm(attrs: Vec<Cw20Attribute>) -> Cw20Event {
        Cw20Event {
            event_type: "wasm".into(),
            attributes: attrs,
        }
    }

    #[test]
    fn registered_transfer_kept() {
        let events = [wasm(vec![
            attr("_contract_address", CL8Y),
            attr("action", "transfer"),
            attr("from", ALICE),
            attr("to", "terra1bob"),
            attr("amount", "1000000000000000000"),
        ])];
        let parsed = parse_cw20_wasm_events(&events, CL8Y, &[ALICE]).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].amount, human_to_raw(1));
        assert_eq!(parsed[0].action, "transfer");
    }

    #[test]
    fn unregistered_flood_ignored() {
        let events = [wasm(vec![
            attr("_contract_address", CL8Y),
            attr("action", "transfer"),
            attr("from", "terra1spam1"),
            attr("to", "terra1spam2"),
            attr("amount", "1"),
        ])];
        let parsed = parse_cw20_wasm_events(&events, CL8Y, &[ALICE]).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn forged_contract_address_ignored() {
        let events = [wasm(vec![
            attr("contract_address", CL8Y),
            attr("_contract_address", "terra1attacker"),
            attr("action", "transfer"),
            attr("from", ALICE),
            attr("to", "terra1bob"),
            attr("amount", "1000"),
        ])];
        let parsed = parse_cw20_wasm_events(&events, CL8Y, &[ALICE]).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn wrong_token_ignored() {
        let other = "terra1othertoken";
        let events = [wasm(vec![
            attr("_contract_address", other),
            attr("action", "transfer"),
            attr("from", ALICE),
            attr("to", "terra1bob"),
            attr("amount", "1"),
        ])];
        let parsed = parse_cw20_wasm_events(&events, CL8Y, &[ALICE]).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn send_and_mint_recognized() {
        let mint = [wasm(vec![
            attr("_contract_address", CL8Y),
            attr("action", "mint"),
            attr("to", ALICE),
            attr("amount", "5"),
        ])];
        let parsed = parse_cw20_wasm_events(&mint, CL8Y, &[ALICE]).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].action, "mint");
        assert!(parsed[0].from.is_empty());
    }
}
