Environment variables we read:
`PAJBOT_TWITTER_CONSUMER_KEY`  
Twitter API Consumer Key  
*REQUIRED*

`PAJBOT_TWITTER_CONSUMER_SECRET`
Twitter API Consumer Secret  
*REQUIRED*

`PAJBOT_TWITTER_ACCESS_TOKEN`  
Twitter API Access Token  
*REQUIRED*

`PAJBOT_TWITTER_ACCESS_TOKEN_SECRET`  
Twitter API Access Token Secret  
*REQUIRED*

`PAJBOT_LISTEN`  
Listen address of the WebSocket server.  
Default value: `127.0.0.1:2356`

`PAJBOT_CONF`  
Path to the .toml config file.  
Default value: `tweet-provider.toml`

`PAJBOT_CACHE`  
Path to the cache file. The cache file will store any Twitter users we've been told to follow, so on next bootup we know to follow them instantly.  
Default value: `cache.json`

`PAJBOT_LOG`  
Log level  
Default value: `INFO`
