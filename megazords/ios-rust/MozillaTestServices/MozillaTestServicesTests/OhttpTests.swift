/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import XCTest

@testable import MozillaTestServices

// These tests cover the integration of the underlying Rust libraries into Swift
// URL{Request,Response} data types, as well as the key management and error
// handling logic of OhttpManager class.

// A testing model of Client, KeyConfigEndpoint, Relay, Gateway, and Target. This
// includes an OHTTP decryption server to decode messages, but does not model TLS,
// etc.
class FakeOhttpNetwork {
    let server = OhttpTestServer()
    let configURL = URL(string: "https://gateway.example.com/ohttp-configs")!
    let relayURL = URL(string: "https://relay.example.com/")!

    // Create an instance of OhttpManager with networking hooks installed to
    // send requests to this model instead of the Internet.
    func newOhttpManager() -> OhttpManager {
        OhttpManager(configUrl: configURL,
                     relayUrl: relayURL,
                     network: client)
    }

    // Response helpers
    func statusResponse(request: URLRequest, statusCode: Int) -> (Data, HTTPURLResponse) {
        (Data(),
         HTTPURLResponse(url: request.url!,
                         statusCode: statusCode,
                         httpVersion: "HTTP/1.1",
                         headerFields: [:])!)
    }

    func dataResponse(request: URLRequest, body: Data, contentType: String) -> (Data, HTTPURLResponse) {
        (body,
         HTTPURLResponse(url: request.url!,
                         statusCode: 200,
                         httpVersion: "HTTP/1.1",
                         headerFields: ["Content-Length": String(body.count),
                                        "Content-Type": contentType])!)
    }

    //
    // Network node models
    //
    func client(_ request: URLRequest) async throws -> (Data, URLResponse) {
        switch request.url {
        case configURL: return config(request)
        case relayURL: return relay(request)
        default: throw NSError()
        }
    }

    func config(_ request: URLRequest) -> (Data, URLResponse) {
        let key = server.getConfig()
        return dataResponse(request: request,
                            body: Data(key),
                            contentType: "application/octet-stream")
    }

    func relay(_ request: URLRequest) -> (Data, URLResponse) {
        return gateway(request)
    }

    func gateway(_ request: URLRequest) -> (Data, URLResponse) {
        let inner = try! server.receive(message: [UInt8](request.httpBody!))

        // Unwrap OHTTP/BHTTP
        var innerUrl = URLComponents()
        innerUrl.scheme = inner.scheme
        innerUrl.host = inner.server
        innerUrl.path = inner.endpoint
        var innerRequest = URLRequest(url: innerUrl.url!)
        innerRequest.httpMethod = inner.method
        innerRequest.httpBody = Data(inner.payload)
        for (k, v) in inner.headers {
            innerRequest.setValue(v, forHTTPHeaderField: k)
        }

        let (innerData, innerResponse) = target(innerRequest)

        // Wrap with BHTTP/OHTTP
        var headers: [String: String] = [:]
        for (k, v) in innerResponse.allHeaderFields {
            headers[k as! String] = v as? String
        }
        let reply = try! server.respond(response: OhttpResponse(statusCode: UInt16(innerResponse.statusCode),
                                                                headers: headers,
                                                                payload: [UInt8](innerData)))
        return dataResponse(request: request,
                            body: Data(reply),
                            contentType: "message/ohttp-res")
    }

    func target(_ request: URLRequest) -> (Data, HTTPURLResponse) {
        // Dummy JSON application response
        let data = try! JSONSerialization.data(withJSONObject: ["hello": "world"])
        return dataResponse(request: request,
                            body: data,
                            contentType: "application/json")
    }
}

class OhttpTests: XCTestCase {
    override func setUp() {
        OhttpManager.keyCache.removeAll()
    }

    // Test that a GET request can retrieve expected data from Target, including
    // passing headers in each direction.
    func testGet() async {
        class DataTargetNetwork: FakeOhttpNetwork {
            override func target(_ request: URLRequest) -> (Data, HTTPURLResponse) {
                XCTAssertEqual(request.url, URL(string: "https://example.com/data")!)
                XCTAssertEqual(request.httpMethod, "GET")
                XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/octet-stream")

                return dataResponse(request: request,
                                    body: Data([0x10, 0x20, 0x30]),
                                    contentType: "application/octet-stream")
            }
        }

        let mock = DataTargetNetwork()
        let ohttp = mock.newOhttpManager()

        let url = URL(string: "https://example.com/data")!
        var request = URLRequest(url: url)
        request.setValue("application/octet-stream", forHTTPHeaderField: "Accept")
        let (data, response) = try! await ohttp.data(for: request)

        XCTAssertEqual(response.statusCode, 200)
        XCTAssertEqual([UInt8](data), [0x10, 0x20, 0x30])
        XCTAssertEqual(response.value(forHTTPHeaderField: "Content-Type"), "application/octet-stream")
    }

    // Test that POST requests to an API using JSON work as expected.
    func testJsonApi() async {
        class JsonTargetNetwork: FakeOhttpNetwork {
            override func target(_ request: URLRequest) -> (Data, HTTPURLResponse) {
                XCTAssertEqual(request.url, URL(string: "https://example.com/api")!)
                XCTAssertEqual(request.httpMethod, "POST")
                XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
                XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "application/json")
                XCTAssertEqual(String(decoding: request.httpBody!, as: UTF8.self),
                               #"{"version":1}"#)

                let data = try! JSONSerialization.data(withJSONObject: ["hello": "world"])
                return dataResponse(request: request,
                                    body: data,
                                    contentType: "application/json")
            }
        }

        let mock = JsonTargetNetwork()
        let ohttp = mock.newOhttpManager()

        let url = URL(string: "https://example.com/api")!
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        request.httpBody = try! JSONSerialization.data(withJSONObject: ["version": 1])
        let (data, response) = try! await ohttp.data(for: request)

        XCTAssertEqual(response.statusCode, 200)
        XCTAssertEqual(String(bytes: data, encoding: .utf8), #"{"hello":"world"}"#)
        XCTAssertEqual(response.value(forHTTPHeaderField: "Content-Type"), "application/json")
    }

    // Test that config keys are cached across requests.
    func testKeyCache() async {
        class CountConfigNetwork: FakeOhttpNetwork {
            var numConfigFetches = 0

            override func config(_ request: URLRequest) -> (Data, URLResponse) {
                numConfigFetches += 1
                return super.config(request)
            }
        }
        let mock = CountConfigNetwork()
        let ohttp = mock.newOhttpManager()

        let request = URLRequest(url: URL(string: "https://example.com/api")!)
        _ = try! await ohttp.data(for: request)
        _ = try! await ohttp.data(for: request)
        _ = try! await ohttp.data(for: request)

        XCTAssertEqual(mock.numConfigFetches, 1)
    }

    // Test that bad key config data throws MalformedKeyConfig error.
    func testBadConfig() async {
        class MalformedKeyNetwork: FakeOhttpNetwork {
            override func config(_ request: URLRequest) -> (Data, URLResponse) {
                dataResponse(request: request,
                             body: Data(),
                             contentType: "application/octet-stream")
            }
        }

        do {
            let mock = MalformedKeyNetwork()
            let ohttp = mock.newOhttpManager()
            let request = URLRequest(url: URL(string: "https://example.com/api")!)
            _ = try await ohttp.data(for: request)
            XCTFail()
        } catch OhttpError.MalformedKeyConfig {
        } catch {
            XCTFail()
        }
    }

    // Test that using the wrong key throws a RelayFailed error and
    // that the key is removed from cache.
    func testWrongKey() async {
        class WrongKeyNetwork: FakeOhttpNetwork {
            override func config(_ request: URLRequest) -> (Data, URLResponse) {
                dataResponse(request: request,
                             body: Data(OhttpTestServer().getConfig()),
                             contentType: "application/octet-stream")
            }

            override func gateway(_ request: URLRequest) -> (Data, URLResponse) {
                do {
                    _ = try server.receive(message: [UInt8](request.httpBody!))
                    XCTFail()
                } catch OhttpError.MalformedMessage {
                } catch {
                    XCTFail()
                }

                return statusResponse(request: request, statusCode: 400)
            }
        }

        do {
            let mock = WrongKeyNetwork()
            let ohttp = mock.newOhttpManager()
            let request = URLRequest(url: URL(string: "https://example.com/")!)
            _ = try await ohttp.data(for: request)
            XCTFail()
        } catch OhttpError.RelayFailed {
        } catch {
            XCTFail()
        }

        XCTAssert(OhttpManager.keyCache.isEmpty)
    }

    // Test that bad Gateway data generates MalformedMessage errors.
    func testBadGateway() async {
        class BadGatewayNetwork: FakeOhttpNetwork {
            override func gateway(_ request: URLRequest) -> (Data, URLResponse) {
                dataResponse(request: request,
                             body: Data(),
                             contentType: "message/ohttp-res")
            }
        }

        do {
            let mock = BadGatewayNetwork()
            let ohttp = mock.newOhttpManager()
            let request = URLRequest(url: URL(string: "https://example.com/api")!)
            _ = try await ohttp.data(for: request)
            XCTFail()
        } catch OhttpError.MalformedMessage {
        } catch {
            XCTFail()
        }
    }

    // Test behaviour when Gateway disallows a Target URL.
    func testDisallowedTarget() async {
        class DisallowedTargetNetwork: FakeOhttpNetwork {
            override func target(_ request: URLRequest) -> (Data, HTTPURLResponse) {
                statusResponse(request: request, statusCode: 403)
            }
        }

        let mock = DisallowedTargetNetwork()
        let ohttp = mock.newOhttpManager()
        let request = URLRequest(url: URL(string: "https://deny.example.com/")!)
        let (_, response) = try! await ohttp.data(for: request)

        XCTAssertEqual(response.statusCode, 403)
    }

    // Test that ordinary network failures are surfaced as URLError.
    func testNetworkFailure() async {
        class NoConnectionNetwork: FakeOhttpNetwork {
            override func client(_ request: URLRequest) async throws -> (Data, URLResponse) {
                if request.url == configURL {
                    return config(request)
                }

                throw NSError(domain: NSURLErrorDomain,
                              code: URLError.cannotConnectToHost.rawValue)
            }
        }

        do {
            let mock = NoConnectionNetwork()
            let ohttp = mock.newOhttpManager()
            let request = URLRequest(url: URL(string: "https://example.com/api")!)
            _ = try await ohttp.data(for: request)
            XCTFail()
        } catch is URLError {
        } catch {
            XCTFail()
        }
    }
}
