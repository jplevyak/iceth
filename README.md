# IC Eth
ETH for the IC.

The IC Eth project realizes a canister smart contract for the Internet Computer blockchain that offers the Ethereum JSON RPC API as an [on-chain API](./iceth-API.md). Requests received on this API by the canister are forwarded to Web2 Ethereum *JSON RPC API services* like [Infura](https://www.infura.io/), [Gateway.fm](https://gateway.fm/), or [CloudFlare](https://www.cloudflare.com/en-gb/web3/) using [HTTPS outcalls](https://internetcomputer.org/docs/current/developer-docs/integrations/http_requests/). This way, the canister acts as a *proxy* to the Web2 world of Ethereum API nodes and simplifies the access to Ethereum JSON RPC API services for canisters. The JSON RPC API exposed by this canister allows a canister smart contract to do much of what a regular Ethereum dApp in the Web2 world could do, e.g., to arbitrarily interact with the Ethereum network, e.g., by querying the state of Ethereum smart contracts or submitting raw transactions to Ethereum.

This canister provides a convenient, yet effective, connection between the Internet Computer and the Ethereum network. For interactions that involve value transfer, such as in the context of X-chain asset transfers, multiple Web2 JSON RPC providers can be queried by a client to increase the assurance of correctness of the answer. This is a decision on the security model that is left to the client.

Authorized principals are permitted to register, update, and de-register so-called *providers*, each of which defines a registered API key for a specific Web2 JSON API service for a given chain id. It furthermore defines the cycles price to be paid when using this provider.

This canister's API can be used in two different modalities depending on the use case:
* *Registered API key:* Client canisters use the canister's RPC API such that canister-registered API keys are used to interact with the Web2 API provider. This has the advantage that the maintainer of the client does not need to manage their own API keys with RPC providers, but simply uses the one registered in the canister.
* *Client-provided API key:* Client canisters can provide their own API key with calls, e.g., to use API providers for which there are no registered providers available. This also helps reduce the quota usage for quota-limited API keys. The API providers to be used this way need to be on an allowlist of the canister.
This gives the canister great flexibility of how it can be used in different deployment scenarios.

At least the following deployment scenarios are supported by the API of this canister:
* The IC community deploys the canister on an IC system subnet and hands control over to the NNS. The canister makes its API available to any canister on the IC as an infrastructure service. At least one API key with high-volume access quota should be registered as a provider in this scenario. Deployment on a system subnet gives the canister IPv4 access, which is currently not available on application subnets, and thus a substantially broader coverage of API providers.
* A project can deploy the canister itself on an application subnet, use the project's own API keys, and limit access to the canister's API to project-specific canisters.
* Anyone can deploy the canister on an application subnet for public use.

**Note**
The canister has been designed to connect to the Ethereum blockchain from the Internet Computer, however, the canister may also be useful to connect to other EVM blockchains that support the same JSON RPC API and follow standards of Ethereum.

The API of the canister is specified through a [Candid interface specification](./iceth.did). Detailed API documentation is available [here](./iceth-API.md).

## Build


### DFX
```bash
dfx build
```

### Docker (reproducable)
```bash
scripts/docker-build
```

## Examples

### local
```bash
dfx canister call --wallet $(dfx identity get-wallet) --with-cycles 600000000 iceth json_rpc_request '("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}","https://cloudflare-eth.com",1000)'
dfx canister call --wallet $(dfx identity get-wallet) --with-cycles 600000000 iceth json_rpc_request '("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}","https://ethereum.publicnode.com",1000)'
dfx canister call iceth register_provider '(record { chain_id=1; service_url="https://cloudflare-eth.com"; api_key="/v1/mainnet"; cycles_per_call=10; cycles_per_message_byte=1; })'
dfx canister call --wallet $(dfx identity get-wallet) --with-cycles 600000000 iceth json_rpc_provider_request '("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}",0,1000)'
```

### mainnet
```bash
dfx canister --network ic call --wallet $(dfx identity --network ic get-wallet) --with-cycles 600000000 iceth json_rpc_request '("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}","https://cloudflare-eth.com",1000)'
dfx canister --network ic call --wallet $(dfx identity --network ic get-wallet) --with-cycles 600000000 iceth json_rpc_request '("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}","https://ethereum.publicnode.com",1000)'
```
