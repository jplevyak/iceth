# IC Eth
ETH for the IC.

## API

[API in Candid](./iceth.did)

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
