use candid::{candid_method, CandidType};
use ic_canister_log::declare_log_buffer;
use ic_canisters_http_types::{
    HttpRequest as IcHttpRequest, HttpResponse as IcHttpResponse, HttpResponseBuilder,
};
use ic_cdk::api::management_canister::http_request::{
    http_request as make_http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod,
    HttpResponse, TransformArgs, TransformContext,
};
use ic_nervous_system_common::{serve_logs, serve_logs_v2, serve_metrics};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{BoundedStorable, DefaultMemoryImpl, StableBTreeMap, Storable};
use std::cell::RefCell;
use std::collections::hash_set::HashSet;

const INGRESS_OVERHEAD_BYTES: u128 = 100;
const INGRESS_MESSAGE_RECEIVED_COST: u128 = 1_200_000u128;
const INGRESS_MESSAGE_BYTE_RECEIVED_COST: u128 = 2_000u128;
const HTTP_OUTCALL_REQUEST_COST: u128 = 400_000_000u128;
const HTTP_OUTCALL_BYTE_RECEIEVED_COST: u128 = 100_000u128;

const STRING_STORABLE_MAX_SIZE: u32 = 100;

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

const ALLOWLIST_REGISTER_API_KEY_LIST: &'static [&'static str] =
    &["jgfvj-q2dnm-hohxf-x5nvm-n3olk-fxbdu-4gfri-4vhci-aztp4-s3k3i-sqe"];

const ALLOWLIST_RPC_LIST: &'static [&'static str] = &[];

type AllowlistSet = HashSet<&'static &'static str>;

declare_log_buffer!(name = INFO, capacity = 1000);
declare_log_buffer!(name = ERROR, capacity = 1000);

#[derive(Default)]
struct Metrics {
    eth_rpc_requests: u64,
    eth_rpc_request_cycles_charged: u64,
    eth_rpc_request_cycles_refunded: u64,
    eth_rpc_request_err_no_permission: u64,
    eth_rpc_request_err_service_url_host_not_allowed: u64,
    eth_rpc_request_err_http_request_error: u64,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
struct StringStorable(String);

impl Storable for StringStorable {
    fn to_bytes(&self) -> std::borrow::Cow<[u8]> {
        // String already implements `Storable`.
        self.0.to_bytes()
    }

    fn from_bytes(bytes: std::borrow::Cow<[u8]>) -> Self {
        Self(String::from_bytes(bytes))
    }
}

impl BoundedStorable for StringStorable {
    const MAX_SIZE: u32 = STRING_STORABLE_MAX_SIZE;
    const IS_FIXED_SIZE: bool = false;
}

thread_local! {
    static METRICS: RefCell<Metrics> = RefCell::new(Metrics::default());
    static ALLOWLIST_SERVICE_HOSTS: RefCell<AllowlistSet> = RefCell::new(AllowlistSet::new());
    static ALLOWLIST_REGISTER_API_KEY: RefCell<AllowlistSet> = RefCell::new(AllowlistSet::new());
    static ALLOWLIST_RPC: RefCell<AllowlistSet> = RefCell::new(AllowlistSet::new());
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static PROVIDERS: RefCell<StableBTreeMap<StringStorable, StringStorable, VirtualMemory<DefaultMemoryImpl>>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(0))),
            )
        );
}

#[derive(CandidType)]
enum EthRpcError {
    NoPermission,
    TooFewCycles(String),
    ServiceUrlParseError,
    ServiceUrlHostMissing,
    ServiceUrlHostNotAllowed,
    HttpRequestError { code: u32, message: String },
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
    }};
}

#[macro_export]
macro_rules! add_metric {
    ($metric:ident, $value:expr) => {{
        METRICS.with(|m| m.borrow_mut().$metric += 1);
    }};
}

#[macro_export]
macro_rules! get_metric {
    ($metric:ident) => {{
        METRICS.with(|m| m.borrow().$metric)
    }};
}

#[ic_cdk_macros::update]
#[candid_method]
async fn eth_rpc_request(
    json_rpc_payload: String,
    service_url: String,
    max_response_bytes: u64,
) -> Result<Vec<u8>, EthRpcError> {
    inc_metric!(eth_rpc_requests);
    let caller = ic_cdk::caller().to_string();
    if !ALLOWLIST_RPC.with(|a| !a.borrow().is_empty() && a.borrow().contains(&caller.as_str())) {
        inc_metric!(eth_rpc_request_err_no_permission);
        return Err(EthRpcError::NoPermission);
    }
    let cycles_available = ic_cdk::api::call::msg_cycles_available128();
    let cost = eth_rpc_cycles_cost(&json_rpc_payload, &service_url, max_response_bytes);
    if cycles_available < cost {
        return Err(EthRpcError::TooFewCycles(format!(
            "requires {} cycles, got {} cycles",
            cost, cycles_available
        )));
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
        inc_metric!(eth_rpc_request_err_service_url_host_not_allowed);
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
        Err((r, m)) => {
            inc_metric!(eth_rpc_request_err_http_request_error);
            Err(EthRpcError::HttpRequestError {
                code: r as u32,
                message: m,
            })
        }
    }
}

fn eth_rpc_cycles_cost(json_rpc_payload: &str, service_url: &str, max_response_bytes: u64) -> u128 {
    let ingress_bytes =
        (json_rpc_payload.len() + service_url.len()) as u128 + INGRESS_OVERHEAD_BYTES;
    let cycles = INGRESS_MESSAGE_RECEIVED_COST
        + INGRESS_MESSAGE_BYTE_RECEIVED_COST * ingress_bytes
        + HTTP_OUTCALL_REQUEST_COST
        + HTTP_OUTCALL_BYTE_RECEIEVED_COST * (ingress_bytes + max_response_bytes as u128);
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
    ALLOWLIST_SERVICE_HOSTS
        .with(|a| (*a.borrow_mut()) = AllowlistSet::from_iter(ALLOWLIST_SERVICE_HOSTS_LIST));
    ALLOWLIST_REGISTER_API_KEY
        .with(|a| (*a.borrow_mut()) = AllowlistSet::from_iter(ALLOWLIST_REGISTER_API_KEY_LIST));
    ALLOWLIST_RPC.with(|a| (*a.borrow_mut()) = AllowlistSet::from_iter(ALLOWLIST_RPC_LIST));
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

    service_compatible(
        CandidSource::Text(&new_interface),
        CandidSource::File(Path::new("iceth.did")),
    )
    .unwrap();
}

#[test]
fn check_eth_rpc_cycles_cost() {
    let base_cost = eth_rpc_cycles_cost(
        "{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}",
        "https://cloudflare-eth.com",
        1000,
    );
    let s10 = "0123456789";
    let base_cost_s10 = eth_rpc_cycles_cost(
        &("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}".to_string()
            + s10),
        "https://cloudflare-eth.com",
        1000,
    );
    assert_eq!(
        base_cost + 10 * (INGRESS_MESSAGE_BYTE_RECEIVED_COST + HTTP_OUTCALL_BYTE_RECEIEVED_COST),
        base_cost_s10
    )
}
