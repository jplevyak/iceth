# IC Eth
ETH for the IC.

## Example

### local
```
dfx canister call --wallet $(dfx identity get-wallet) --with-cycles 500000000 iceth ethRpcRequest '("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}","https://cloudflare-eth.com",1000)'
```

### mainnet
```
dfx canister --network ic call --wallet $(dfx identity --network ic get-wallet) --with-cycles 500000000 iceth ethRpcRequest '("{\"jsonrpc\":\"2.0\",\"method\":\"eth_gasPrice\",\"params\":[],\"id\":1}","https://cloudflare-eth.com",1000)'
```
