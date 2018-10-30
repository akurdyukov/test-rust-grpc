# Simple Rust gRPC demo

Shows client -> user-based processor -> subscription stream way.

Run server with
```
cargo run --bin server
```

Run client with
```
cargo run --bin client
```

Also, may use `grpcc` like this:

```
grpcc -i -p ./src/protos/example/service.proto --address 127.0.0.1:9000 --eval 'client.execute({command_id: 1}, printReply)'
```

## TODO

[ ] Correct client unsubscription
[ ] Sometimes server does not answer, fix that
[ ] Use headers for user_id
