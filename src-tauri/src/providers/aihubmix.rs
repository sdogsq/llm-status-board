//! AiHubMix Key 余额直接以美元返回；累计用量以美分返回。

use crate::http;
use crate::types::BalanceSource::AiHubMixRemain;
use crate::types::{AccountConfig, AccountInfo, BalanceSnapshot, Money, RefreshOutcome};
use serde_json::{json, Value};

// AiHubMix can report this compatibility limit even when usable credits are far lower.
const COMPATIBILITY_LIMIT_USD: f64 = 100_000_000.0;
// /remain uses -1 internal quota unit (-1 / 500,000 USD) for an unlimited key.
const UNLIMITED_REMAINING: f64 = -0.000_002;

pub async fn fetch(client: &reqwest::Client, acc: &AccountConfig) -> Result<AccountInfo, String> {
    let key = acc.api_key.trim();
    if key.is_empty() {
        return Err("需要填写 AiHubMix API Key".into());
    }
    let base = acc.base();
    let (header, value) = http::bearer(key);
    let headers = [
        (header.as_str(), value),
        ("Accept", "application/json".to_string()),
    ];
    let remain_url = format!("{base}/dashboard/billing/remain");
    let subscription_url = format!("{base}/dashboard/billing/subscription");
    let usage_url = format!("{base}/dashboard/billing/usage");
    let (remaining, subscription, usage) = tokio::join!(
        http::send_json(client, "GET", &remain_url, &headers, None),
        http::send_json(client, "GET", &subscription_url, &headers, None),
        http::send_json(client, "GET", &usage_url, &headers, None),
    );
    let remaining = remaining.map_err(|e| format!("AiHubMix 查询 Key 剩余额度失败: {e}"))?;
    parse_billing(remaining, subscription.ok(), usage.ok())
}

fn billing_amount(response: &Value, field: &str) -> Option<f64> {
    let amount = match response.get(field) {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    amount.filter(|n| n.is_finite())
}

fn is_compatibility_limit(total: Option<f64>) -> bool {
    total == Some(COMPATIBILITY_LIMIT_USD)
}

/// Match old AiHubMix-only observations that used the compatibility limit as credit.
pub(crate) fn is_invalid_legacy_snapshot(snapshot: &BalanceSnapshot) -> bool {
    snapshot.balance_source.is_none() && is_compatibility_limit(snapshot.total)
}

/// Reverse the legacy formula for old AiHubMix snapshots. Direct balances cannot
/// be subtracted from a spending limit to recover usage.
pub(crate) fn prepare_history_snapshot(snapshot: &mut BalanceSnapshot) {
    if snapshot.balance_source.is_some() {
        return;
    }
    if snapshot.currency == "USD" && snapshot.used.is_none() {
        snapshot.used = snapshot
            .total
            .zip(snapshot.balance)
            .map(|(total, balance)| total - balance)
            .filter(|used| used.is_finite() && *used >= 0.0);
    }
    if is_invalid_legacy_snapshot(snapshot) {
        snapshot.balance = None;
        snapshot.total = None;
    }
}

/// Old cached results with the compatibility limit must wait for a new balance query.
pub(crate) fn invalidate_legacy_balance(outcome: &mut RefreshOutcome) {
    let Some(info) = &outcome.info else { return };
    let legacy = info.balance_source.is_none()
        && info.raw.as_ref().is_some_and(|raw| {
            raw.get("usage").is_some()
                && raw.get("subscription").is_some_and(|sub| {
                    is_compatibility_limit(billing_amount(sub, "hard_limit_usd"))
                })
        });
    if legacy {
        outcome.ok = false;
        outcome.info = None;
        outcome.error = Some("AiHubMix 旧余额使用了兼容上限，请刷新以查询 Key 剩余额度。".into());
    }
}

fn parse_billing(
    remaining: Value,
    subscription: Option<Value>,
    usage: Option<Value>,
) -> Result<AccountInfo, String> {
    let remaining_usd = billing_amount(&remaining, "total_usage")
        .ok_or_else(|| "AiHubMix 剩余额度响应缺少有效的 total_usage".to_string())?;
    let unlimited = remaining_usd == UNLIMITED_REMAINING;
    let total = subscription
        .as_ref()
        .and_then(|sub| billing_amount(sub, "hard_limit_usd"));
    let used = usage
        .as_ref()
        .and_then(|usage| billing_amount(usage, "total_usage"))
        .filter(|n| *n >= 0.0)
        .map(|cents| cents / 100.0);
    let total = total.filter(|n| !unlimited && *n >= 0.0 && !is_compatibility_limit(Some(*n)));
    let usd = |amount| Money {
        amount,
        currency: "USD".into(),
    };
    Ok(AccountInfo {
        balance: (!unlimited).then(|| usd(remaining_usd)),
        balance_source: Some(AiHubMixRemain),
        used: used.map(usd),
        total: total.map(usd),
        note: unlimited.then(|| "此 Key 无额度上限。账户余额请在 AiHubMix 控制台查看。".into()),
        raw: Some(json!({ "remain": remaining, "subscription": subscription, "usage": usage })),
        fetched_at: crate::types::now_iso(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProviderKind;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn recovers_only_usage_that_can_be_proven_from_the_legacy_formula() {
        let mut snapshot: BalanceSnapshot = serde_json::from_value(json!({
            "ts": "2026-11-17T12:00:00Z", "accountId": "a", "currency": "USD",
            "total": 100_000_000.0, "balance": 99_966_360.0
        }))
        .unwrap();
        let mut with_direct_balance = snapshot.clone();
        with_direct_balance.balance_source = Some(AiHubMixRemain);
        prepare_history_snapshot(&mut with_direct_balance);
        assert_eq!(with_direct_balance.used, None);

        prepare_history_snapshot(&mut snapshot);
        assert_eq!(snapshot.used, Some(33_640.0));
        assert_eq!(snapshot.balance, None);
        assert_eq!(snapshot.total, None);
    }

    #[test]
    fn uses_the_reported_balance_and_keeps_billing_fields_independent() {
        let info = parse_billing(
            json!({"object": "list", "total_usage": 1535.257862}),
            Some(json!({"hard_limit_usd": 3000.0})),
            Some(json!({"total_usage": 100_000.0})),
        )
        .unwrap();
        let balance = info.balance.unwrap();
        assert_eq!(balance.amount, 1535.257862);
        assert_eq!(balance.currency, "USD");
        assert_eq!(info.used.unwrap().amount, 1000.0);
        assert_eq!(info.total.unwrap().amount, 3000.0);
        assert_eq!(info.balance_source, Some(AiHubMixRemain));
    }

    #[test]
    fn accepts_zero_and_overdrawn_balances_and_handles_the_unlimited_marker() {
        for (remaining, expected) in [
            (" 0 ", Some(0.0)),
            ("-0.5", Some(-0.5)),
            ("-0.000002", None),
        ] {
            let info = parse_billing(json!({"total_usage": remaining}), None, None).unwrap();
            assert_eq!(info.balance.map(|amount| amount.amount), expected);
            assert_eq!(info.note.is_some(), expected.is_none());
        }
    }

    #[test]
    fn rejects_invalid_balances_even_when_the_old_inputs_are_available() {
        let subscription = json!({"hard_limit_usd": 10});
        let usage = json!({"total_usage": 100});
        for bad in [
            Value::Null,
            json!(true),
            json!([]),
            json!(""),
            json!("NaN"),
            json!("1e999"),
        ] {
            let error = parse_billing(
                json!({"total_usage": bad}),
                Some(subscription.clone()),
                Some(usage.clone()),
            )
            .unwrap_err();
            assert!(error.contains("total_usage"), "{error}");
        }
    }

    #[test]
    fn old_compatibility_limit_cache_requires_a_refresh() {
        let mut result: RefreshOutcome = serde_json::from_value(json!({
            "accountId": "a", "ok": true, "error": null,
            "info": {
                "balance": {"amount": 99_999_999.0, "currency": "USD"},
                "raw": {"subscription": {"hard_limit_usd": 100_000_000.0}, "usage": {"total_usage": 100}},
                "extras": [], "fetchedAt": "2026-11-18T00:00:00Z"
            }
        })).unwrap();
        invalidate_legacy_balance(&mut result);
        assert!(!result.ok);
        assert!(result.info.is_none());
        assert!(result.error.unwrap().contains("请刷新"));
    }

    #[test]
    fn provider_kind_round_trips_in_account_configuration() {
        let acc = AccountConfig {
            kind: ProviderKind::AiHubMix,
            ..Default::default()
        };
        let saved = serde_json::to_value(&acc).unwrap();
        assert_eq!(saved["kind"], "aiHubMix");
        let restored: AccountConfig = serde_json::from_value(saved).unwrap();
        assert_eq!(restored.kind, ProviderKind::AiHubMix);
    }

    fn read_billing_request(stream: &mut TcpStream) -> String {
        // macOS can inherit the nonblocking listener mode on accepted sockets.
        // Request headers may arrive after accept(), so use bounded blocking I/O.
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.ends_with(b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0, "Incomplete billing request");
            request.extend_from_slice(&buffer[..count]);
            assert!(request.len() < 16384);
        }
        String::from_utf8(request).unwrap()
    }

    #[test]
    fn accepted_nonblocking_connection_waits_for_fragmented_request_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        // Reproduce the accepted-socket mode on every OS, without public network.
        stream.set_nonblocking(true).unwrap();
        let request = "GET /dashboard/billing/subscription HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            client
                .write_all(b"GET /dashboard/billing/subscription HTTP/1.1\r\n")
                .unwrap();
            thread::sleep(Duration::from_millis(30));
            client.write_all(b"Host: localhost\r\n\r\n").unwrap();
        });
        assert_eq!(read_billing_request(&mut stream), request);
        writer.join().unwrap();
    }

    // A local HTTP fixture checks real request paths, headers, status handling,
    // and dispatch without provider credentials or external network access.
    fn billing_server(
        remain_status: u16,
        subscription_status: u16,
        usage_status: u16,
    ) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            let deadline = Instant::now() + Duration::from_secs(5);
            while requests.len() < 3 {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "Billing requests did not arrive");
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("Local billing server failed: {e}"),
                };
                let request = read_billing_request(&mut stream);
                let (status, body) = match request.lines().next().unwrap() {
                    "GET /dashboard/billing/remain HTTP/1.1" => (
                        remain_status,
                        json!({ "object": "list", "total_usage": 1535.257862 }),
                    ),
                    "GET /dashboard/billing/subscription HTTP/1.1" => (
                        subscription_status,
                        json!({ "hard_limit_usd": 100_000_000 }),
                    ),
                    "GET /dashboard/billing/usage HTTP/1.1" => {
                        (usage_status, json!({ "total_usage": 2937045.8762 }))
                    }
                    other => panic!("Unexpected billing request: {other}"),
                };
                let body = if status == 200 {
                    body.to_string()
                } else {
                    json!({ "error": { "message": "billing request rejected" } }).to_string()
                };
                write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                requests.push(request);
            }
            requests
        });
        (base, handle)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    #[tokio::test]
    async fn fetches_key_quota_with_bearer_and_hides_the_compatibility_limit() {
        let (base, server) = billing_server(200, 200, 200);
        let acc = AccountConfig {
            kind: ProviderKind::AiHubMix,
            base_url: format!(" {base}/ "),
            api_key: " test-aihubmix-key ".into(),
            ..Default::default()
        };
        let info = crate::providers::fetch_account(&client(), &acc)
            .await
            .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 3);
        for request in requests {
            assert!(request
                .to_ascii_lowercase()
                .contains("\r\nauthorization: bearer test-aihubmix-key\r\n"));
            assert!(request
                .to_ascii_lowercase()
                .contains("\r\naccept: application/json\r\n"));
        }
        assert_eq!(info.balance.unwrap().amount, 1535.257862);
        assert_eq!(info.used.unwrap().amount, 29370.458762);
        assert!(info.total.is_none());
    }

    #[tokio::test]
    async fn only_a_failure_to_fetch_remaining_quota_fails_the_balance_query() {
        for (remain, subscription, usage) in [(200, 503, 503), (401, 200, 200)] {
            let (base, server) = billing_server(remain, subscription, usage);
            let account = AccountConfig {
                kind: ProviderKind::AiHubMix,
                base_url: base,
                api_key: "test-aihubmix-key".into(),
                ..Default::default()
            };
            let result = fetch(&client(), &account).await;
            server.join().unwrap();
            if remain == 200 {
                let info = result.unwrap();
                assert_eq!(info.balance.unwrap().amount, 1535.257862);
                assert!(info.used.is_none() && info.total.is_none());
            } else {
                let error = result.unwrap_err();
                assert!(error.contains("查询 Key 剩余额度失败"), "{error}");
                assert!(error.contains("billing request rejected"), "{error}");
            }
        }
    }

    #[tokio::test]
    async fn rejects_blank_api_key_before_making_requests() {
        let acc = AccountConfig {
            kind: ProviderKind::AiHubMix,
            api_key: " \t ".into(),
            ..Default::default()
        };
        assert_eq!(
            fetch(&client(), &acc).await.unwrap_err(),
            "需要填写 AiHubMix API Key"
        );
    }
}
