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

```json5
{ "type": "ack_subscriptions", "data": [123456, 234567] }
{ "type": "protocol_error", "data": "missing field `type` at line 1 column 2" }
{ "type": "tweet", "data": {
    "text": "Adjfkdkoo",
    "id": 1218503583311769600,
    "created_at": 1579348867,
    "user": {
        "id": 81085011,
        "screen_name": "pajtest",
        "name": "paj pajsson"
    },
    "truncated": false, // if tweet was truncated to 140 characters for compatibility
    "in_reply_to_user_id": null, // or number
    "in_reply_to_screen_name": null, // or string
    "in_reply_to_status_id": null, // or number
    "urls": [{
        "url": "t.co/dank",
        "display_url": "https://google.com",
        "expanded_url": null, // or string
        "range_start": 0,
        "range_end": 9,
    }]
}}
```
