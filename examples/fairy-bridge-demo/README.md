# Fairy bridge demo

This example is meant to demonstrate the fairy-bridge library.

## Usage

`./run-demo.py`

This will perform a request against the `https://httpbin.org/anything` endpoint and printout the
response.

Arguments:

  * `--python`: Use the Python implemented backend (default is the reqwest backend)
  * `--sync`: Run the request in sync mode (default is async mode)
  * `--post`: Perform a `POST` request (default is `GET`)
  * `--conn-timeout CONN_TIMEOUT`: set the connection timeout
  * `--timeout TIMEOUT`: set the total request timeout
  * `--redirect-limit REDIRECT_LIMIT`: set the redirect limits

## Example output

### ./run-demo.py

Performs a GET request using the reqwest backend

```
GET https://httpbin.org/anything (async)
got response
status: 200
response:
{
  "args": {}, 
  "data": "", 
  "files": {}, 
  "form": {}, 
  "headers": {
    "Accept": "*/*", 
    "Host": "httpbin.org", 
    "User-Agent": "fairy-bridge-demo", 
    "X-Amzn-Trace-Id": "Root=1-65848c2f-46df949b3229b84833aa445f", 
    "X-Foo": "bar"
  }, 
  "json": null, 
  "method": "GET", 
  "origin": "8.9.85.40", 
  "url": "https://httpbin.org/anything"
}
```


### ./run-demo.py --python --post --sync

Perform a POST request using the Python backend in a non-async context.

```
POST https://httpbin.org/anything (sync)
got response
status: 200
response:
{
  "args": {}, 
  "data": "{\"guid\":\"abcdef1234\",\"foo\":\"Bar\"}", 
  "files": {}, 
  "form": {}, 
  "headers": {
    "Accept": "*/*", 
    "Accept-Encoding": "gzip, deflate", 
    "Content-Length": "33", 
    "Content-Type": "application/json", 
    "Host": "httpbin.org", 
    "User-Agent": "fairy-bridge-demo", 
    "X-Amzn-Trace-Id": "Root=1-65848ca7-2017c7cf112af7fa76c9c2e7", 
    "X-Foo": "bar"
  }, 
  "json": {
    "foo": "Bar", 
    "guid": "abcdef1234"
  }, 
  "method": "POST", 
  "origin": "8.9.85.40", 
  "url": "https://httpbin.org/anything"
}
```

### ./run-demo.py --conn-timeout 0

Perform a GET request with a 0 ms timeout to force a failure

```
GET http://httpbin.org/anything (async)
error: BackendError(reqwest error: error sending request for url (http://httpbin.org/anything): error trying to connect: operation timed out)
```
