use candid::{candid_method, CandidType};
use ic_cdk::api::management_canister::http_request::{
    http_request as make_http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
    TransformContext,
};
use ic_canisters_http_types::{
    HttpRequest as IcHttpRequest, HttpResponse as IcHttpResponse, HttpResponseBuilder,
};
use ic_nervous_system_common::{
    serve_logs, serve_logs_v2, serve_metrics
};
use ic_canister_log::declare_log_buffer;
use std::collections::hash_set::HashSet;
use std::cell::RefCell;

const INGRESS_OVERHEAD: u128 = 100;

const ALLOWLIST_SERVICE_HOSTS_LIST: &'static [&'static str] = &[
    "cloudflare-eth.com",
    "ethereum.publicnode.com",
    "eth-mainnet.g.alchemy.com",
    "eth-goerli.g.alchemy.com",
    "rpc.flashbots.net",
    "eth-mainnet.blastapi.io",
    "ethereumnodelight.app.runonflux.io",
    "eth.nownodes.io",
    "rpc.ankr.com/eth_goerli",
    "mainnet.infura.io",
    "eth.getblock.io",
    "api.0x.org",
    "erigon-mainnet--rpc.datahub.figment.io",
    "archivenode.io",
    "nd-6eaj5va43jggnpxouzp7y47e4y.ethereum.managedblockchain.us-east-1.amazonaws.com",
    "eth-mainnet.nodereal.io",
    "ethereum-mainnet.s.chainbase.online",
    "eth.llamarpc.com",
    "ethereum-mainnet-rpc.allthatnode.com",
    "api.zmok.io",
    "in-light.eth.linkpool.iono",
    "api.mycryptoapi.com",
    "mainnet.eth.cloud.ava.dono",
    "eth-mainnet.gateway.pokt.network",
];

type AllowlistSet = HashSet<&'static &'static str>;

declare_log_buffer!(name = INFO, capacity = 1000);
declare_log_buffer!(name = ERROR, capacity = 1000);

#[derive(Default)]
struct Metrics {
    eth_rpc_requests: u64,
    eth_rpc_request_cycles_charged: u64,
    eth_rpc_request_cycles_refunded: u64,
}

thread_local! {
    static ALLOWLIST_SERVICE_HOSTS: RefCell<AllowlistSet> = RefCell::new(AllowlistSet::new());
        static METRICS: RefCell<Metrics> = RefCell::new(Metrics::default());
}

#[derive(CandidType)]
enum EthRpcError {
    TooFewCycles(String),
    ServiceUrlParseError,
    ServiceUrlHostMissing,
    ServiceUrlHostNotAllowed,
    HttpRequestError {
        code: u32,
        message: String,
    },
}

#[macro_export]
macro_rules! c_log {
	($sink:expr, $message:expr $(,$args:expr)* $(,)*) => {{
		let message = std::format!($message $(,$args)*);
		// Print the message for convenience for local development (e.g. integration tests)
		ic_cdk::println!("{}", &message);
		ic_canister_log::log!($sink, $message $(,$args)*);
	}}
}

#[macro_export]
macro_rules! inc_metric {
	($metric:ident) => {{
        METRICS.with(|m| m.borrow_mut().$metric += 1);
	}}
}

#[macro_export]
macro_rules! add_metric {
	($metric:ident, $value:expr) => {{
        METRICS.with(|m| m.borrow_mut().$metric += 1);
	}}
}

#[macro_export]
macro_rules! get_metric {
	($metric:ident) => {{
        METRICS.with(|m| m.borrow().$metric)
	}}
}

#[ic_cdk_macros::update(name = "ethRpcRequest")]
#[candid_method(update, rename = "ethRpcRequest")]
async fn eth_rpc_request(
    json_rpc_payload: String,
    service_url: String,
    max_response_bytes: u64,
) -> Result<Vec<u8>, EthRpcError> {
    inc_metric!(eth_rpc_requests);
    let cycles_available = ic_cdk::api::call::msg_cycles_available128();
    let cost = eth_rpc_cycles_cost(&json_rpc_payload, &service_url, max_response_bytes);
    if cycles_available < cost {
        return Err(EthRpcError::TooFewCycles(format!("requires {} cycles, got {} cycles", cost, cycles_available)));
    }
    ic_cdk::api::call::msg_cycles_accept128(cost);
    add_metric!(eth_rpc_request_cycles_charged, cost);
    add_metric!(eth_rpc_request_cycles_charged, cycles_available - cost);
    let parsed_url = url::Url::parse(&service_url).or(Err(EthRpcError::ServiceUrlParseError))?;
    let host = parsed_url
        .host_str()
        .ok_or(EthRpcError::ServiceUrlHostMissing)?
        .to_string();
    if !ALLOWLIST_SERVICE_HOSTS.with(|a| a.borrow().contains(&host.as_str())) {
        return Err(EthRpcError::ServiceUrlHostNotAllowed);
    }
    let request_headers = vec![
        HttpHeader {
            name: "Content-Type".to_string(),
            value: "application/json".to_string(),
        },
        HttpHeader {
            name: "Host".to_string(),
            value: host.to_string(),
        },
    ];
    let request = CanisterHttpRequestArgument {
        url: service_url,
        max_response_bytes: Some(max_response_bytes),
        method: HttpMethod::POST,
        headers: request_headers,
        body: Some(json_rpc_payload.as_bytes().to_vec()),
        transform: Some(TransformContext::new(transform, vec![])),
    };
    match make_http_request(request).await {
        Ok((result,)) => Ok(result.body),
        Err((r, m)) => Err(EthRpcError::HttpRequestError { code: r as u32, message: m }),
    }
}

fn eth_rpc_cycles_cost(json_rpc_payload: &str, service_url: &str, max_response_bytes: u64) -> u128 {
    let ingress_bytes = (json_rpc_payload.len() + service_url.len()) as u128 + INGRESS_OVERHEAD;
    let cycles = 
        // 1.2M for an ingress message received
        1_200_000u128
        // 2K per ingress message byte received
        + 2_000u128 as u128 * ingress_bytes
        // 400M for the HTTPS outcall request
        + 400_000_000u128
        // 100K per byte of ingress message which is the size of http request size plus some overhead
        + 100_000u128 as u128 * ingress_bytes + max_response_bytes as u128;
    cycles
}

#[ic_cdk_macros::query(name = "transform")]
fn transform(args: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: args.response.status.clone(),
        body: args.response.body,
        // Strip headers as they contain the Date which is not necessarily the same
        // and will prevent consensus on the result.
        headers: Vec::<HttpHeader>::new(),
    }
}

#[ic_cdk_macros::init]
fn init() {
    ALLOWLIST_SERVICE_HOSTS.with(|a| (*a.borrow_mut()) = AllowlistSet::from_iter(ALLOWLIST_SERVICE_HOSTS_LIST));
}

#[ic_cdk::query]
fn http_request(request: IcHttpRequest) -> IcHttpResponse {
    match request.path() {
        "/metrics" => serve_metrics(encode_metrics),
        "/logs" => serve_logs_v2(request, &INFO, &ERROR),
        "/log/info" => serve_logs(&INFO),
        "/log/error" => serve_logs(&ERROR),
        _ => HttpResponseBuilder::not_found().build(),
    }
}

/// Encode the metrics in a format that can be understood by Prometheus.
fn encode_metrics(w: &mut ic_metrics_encoder::MetricsEncoder<Vec<u8>>) -> std::io::Result<()> {
	w.encode_gauge(
		"canister_version",
        ic_cdk::api::canister_version() as f64,
		"Canister version.",
		)?;
	w.encode_gauge(
		"stable_memory_pages",
        ic_cdk::api::stable::stable64_size() as f64,
		"Size of the stable memory allocated by this canister measured in 64K Wasm pages.",
		)?;
	w.encode_counter(
		"eth_rpc_requests",
        get_metric!(eth_rpc_requests) as f64,
		"Number of eth_rpc_request() calls.",
		)?;
	w.encode_counter(
		"eth_rpc_request_cycles_charged",
        get_metric!(eth_rpc_request_cycles_charged) as f64,
		"Cycles charged by eth_rpc_request() calls.",
		)?;
	w.encode_counter(
		"eth_rpc_request_cycles_refunded",
        get_metric!(eth_rpc_request_cycles_refunded) as f64,
		"Cycles refunded by eth_rpc_request() calls.",
		)?;
	Ok(())
}

#[cfg(not(any(target_arch = "wasm32", test)))]
fn main() {
    candid::export_service!();
    std::print!("{}", __export_service());
}

#[cfg(any(target_arch = "wasm32", test))]
fn main() {}

#[test]
fn check_candid_interface() {
    use candid::utils::{service_compatible, CandidSource};
    use std::path::Path;

    candid::export_service!();
    let new_interface = __export_service();

    service_compatible( CandidSource::Text(&new_interface), CandidSource::File(Path::new("iceth.did")),).unwrap();
}
