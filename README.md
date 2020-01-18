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
{ "type": "tweet", "data": {"text":"Adjfkdkoo","id":1218503583311769600,"created_at":1579348867,"text":"Adjfkdkoo","user":{"id":81085011,"screen_name":"pajtest","name":"paj pajsson"},"truncated":false,"in_reply_to_user_id":null,"in_reply_to_screen_name":null,"in_reply_to_status_id":null,"urls":[]}}
```
