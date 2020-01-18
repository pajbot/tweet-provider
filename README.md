# tweet-provider

## Websocket

- Pings every 30 seconds
- Drops connection if no client message for 90 seconds

### API

#### From Client

```json
{ "type": "set_subscriptions", "data": [123456, 234567] }
{ "type": "insert_subscriptions", "data": [123456, 234567] }
{ "type": "remove_subscriptions", "data": [123456, 234567] }
{ "type": "exit" }
```

#### From Server

```json
{ "type": "ack_subscriptions", "data": [123456, 234567] }
{ "type": "protocol_error", "data": "missing field `type` at line 1 column 2" }
{ "type": "tweet", "data": TODO }
```
