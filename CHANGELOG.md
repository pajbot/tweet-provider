# Changelog

## Unversioned

## [0.1.2] - 2020-07-09

- Add a configuration option to always restart the twitter stream consumer whenever the set of requested follows changes, as opposed to only doing it when changes are additive. In the configuration file: `twitter.always_restart = true`, in command line arguments: `--twitter-always-restart`, in environment variables: `PAJBOT_TWITTER_ALWAYS_RESTART=<anything>`. (#20)
- Send close frames when the application shuts down (#21)

## [0.1.1] - 2020-07-04

- Initial release