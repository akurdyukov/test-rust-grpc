

```
grpcc -i -p ./src/protos/example/service.proto --address 127.0.0.1:9000 --eval 'client.execute({command_id: 1}, printReply)'
```
