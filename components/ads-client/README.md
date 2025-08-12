# Ads Client

## Overview

Mozilla Ads Client (also referred to as "MAC") is a library that handles integration with the Mozilla Ads Routing Service (MARS). It is primarily intended for use on mobile surfaces, namely Firefox iOS and Android, but can be used by any surface able to ingest components from Application Services.

The Ads Client component can request and display standard ad placements, and calls the appropriate callback URLs to send anonymized impressions and clicks back to Mozilla. MAC also provides a facility to report user dissatisfaction with ads so we can take appropriate action as necessary.

Like MARS, Ads Client is privacy-first. It does not track user information and it does not send sensitive identifiable information to Mozilla. Of the information Mozilla does receive, anything shared with advertisers is aggregated and/or de-identified to preserve user privacy.

While we welcome outside feedback and are committed to open source, this library is intended solely for use on Mozilla properties.

## Tests

Tests are run with

```shell
cargo test -p ads-client
```
