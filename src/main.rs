use candid::candid_method;
use ic_cdk::api::management_canister::http_request::{
    http_request, CanisterHttpRequestArgument, HttpHeader, HttpMethod, HttpResponse, TransformArgs,
    TransformContext,
};

const INGRESS_OVERHEAD: u128 = 100;

const ALLOWLIST_SERVICE_HOSTS: &'static [&'static str] = &[
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

#[ic_cdk_macros::update(name = "ethRpcRequest")]
#[candid_method(update, rename = "ethRpcRequest")]
async fn eth_rpc_request(
    json_rpc_payload: String,
    service_url: String,
    max_response_bytes: u64,
) -> Result<Vec<u8>, String> {
    let cycles_available = ic_cdk::api::call::msg_cycles_available128();
    let cost = eth_rpc_cycles_cost(&json_rpc_payload, &service_url, max_response_bytes);
    if cycles_available < cost {
        return Err(format!("requires {} cycles, got {} cycles", cycles_available, cost));
    }
    ic_cdk::api::call::msg_cycles_accept128(cost);
    let parsed_url = url::Url::parse(&service_url).or(Err("unable to parse serviceUrl"))?;
    let host = parsed_url
        .host_str()
        .ok_or("unable to get host from serviceUrl".to_string())?
        .to_string();
    if !ALLOWLIST_SERVICE_HOSTS.contains(&host.as_str()) {
        return Err(format!("host {host} not on allowlist"));
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
    match http_request(request).await {
        Ok((result,)) => Ok(result.body),
        Err((r, m)) => Err(format!("http_request error {r:?}: {m}")),
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
#[candid_method(query, rename = "transform")]
fn transform(args: TransformArgs) -> HttpResponse {
    HttpResponse {
        status: args.response.status.clone(),
        body: args.response.body,
        // Strip headers as they contain the Date which is not necessarily the same
        // and will prevent consensus on the result.
        headers: Vec::<HttpHeader>::new(),
    }
}

#[cfg(not(any(target_arch = "wasm32", test)))]
fn main() {
    candid::export_service!();
    std::print!("{}", __export_service());
}

#[cfg(any(target_arch = "wasm32", test))]
fn main() {}
