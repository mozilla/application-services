#include <cstdint>

namespace fairy_bridge {

struct BackendSettings {
    uint32_t timeout;
    uint32_t connect_timeout;
    uint32_t redirect_limit;
};


enum class Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
    Patch,
};

struct Header {
    const char* key;
    const char* value;
};

struct Request {
    Method method;
    const char* url;
    Header* headers;
    size_t header_count;
    const char* body;
    size_t body_length;
};

/**
 * Opaque HTTP result type
 */
struct Result;

/**
 * Calls used to build up a Result.
 *
 * Strings are passed as (const char*, size_t), pairs since this is often easier for backends to work with.
 */
extern "C" {
    void fairy_bridge_result_set_url(Result* result, const char* url, size_t length);
    void fairy_bridge_result_set_status_code(Result* result, uint16_t code);
    void fairy_bridge_result_add_header(Result* result, const char* key, size_t key_length, const char* value, size_t value_length);
    void fairy_bridge_result_extend_body(Result* result, const char* data, size_t length);
}

/**
 * Complete a result
 *
 * Call this after the result has been successfully built using the previous methods.  This
 * consumes the result pointer and it should not be used again by the backend.
 */
extern "C" {
    void fairy_bridge_result_complete(Result* result);
}

/**
 * Complete a result with an error
 *
 * This causes an error to be returned for the result.  Any previous builder calls will be
 * ignored.  This consumes the result pointer and it should not be used again by the backend.
 */
extern "C" {
    void fairy_bridge_result_complete_error(Result* result, const char* message, size_t length);
}

} // namespace fairy_bridge

/**
 * Backend API
 *
 * This must be implemented by the backend code.
 */
extern "C" {
    /**
     * Initialize the backend.  This is called once at startup.
     */
    void fairy_bridge_backend_c_init(fairy_bridge::BackendSettings settings);

    /**
     * Perform a rquest
     *
     * The backend should schedule the request to be performed in a separate thread.
     *
     * The result is initially empty.  It should be built up and completed by the
     * `fairy_bridge_result_*` functions.
     *
     * `request` and `result` are valid until `fairy_bridge_result_complete` or
     * `fairy_bridge_result_complete_error` is called.  After that they should not be used.
     */
    void fairy_bridge_backend_c_send_request(fairy_bridge::Request* request, fairy_bridge::Result* result);
}
