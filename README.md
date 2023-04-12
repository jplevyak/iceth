# IC Eth
ETH for the IC.

## Example

```
dfx canister call --wallet $(dfx identity get-wallet) --with-cycles 500000000 $(dfx canister id iceth) ethRpcRequest ("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}","https://cloudflare-eth.com",1000)
```
