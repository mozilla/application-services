# Life of a sync

Each sync goes through a sequence of states, modeled as a state machine. These states handle authentication, fetching encryption keys, pulling new changes from the server and local store, merging, and updating the server and store.

## 1. Authentication

### 1.1 Get an OAuth token

The first step requires authenticating with [Firefox Accounts](https://mozilla.github.io/application-services/docs/accounts/welcome.html) to obtain an **OAuth token** and **Sync encryption keys**. If we already have a token and keys, we can skip ahead to 1.2, and exchange it for a token server token.

- What if we have a token but no keys?
- Describe the [encryption scheme](https://github.com/mozilla/fxa-auth-server/wiki/onepw-protocol).

### 1.2 Exchange the OAuth token for a token server token

The [token server](https://mozilla-services.readthedocs.io/en/latest/token/index.html) handles **node assignment**, so we know which storage node to talk to, and **token generation**, so we can authenticate to that node.

- Explain node reassignment.

*Legacy clients also support authentication with signed BrowserID assertions, but this flow is deprecated and intentionally undocumented.*

## 2. Setup

At this point, we have our token server token, and can make authenticated requests to our storage node.

### 1.1 Fetch `info/collections`

The `info/collections` endpoint returns last-modified times for all **collections**. Collections hold records called BSOs. Each BSO is a JSON string with an ID, modified timestamp, and opaque payload. Except for the special `meta/global` record discussed in the next step, the payload is always encrypted. Decrypting the payload yields _another_ JSON string, containing the record contents.

### 1.2 Fetch or upload `meta/global`

The `meta/global` record holds sync IDs, storage versions, and collections that we declined to sync.

- Explain disabled vs. explicitly declined engines.
- Explain why `m/g` needs to mention all possible engines, even ones we don't implement.

### 1.3 Fetch or upload `crypto/keys`

The `crypto/keys` record holds collection encryption keys. This collection is encrypted with kB.

## 3. Sync

- Batch downloading and uploading.

