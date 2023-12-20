#include <string>
#include <string.h>
#include <thread>
#include "curl/curl.h"
#include "fairy_bridge.h"

std::string copy_ffi_header(fairy_bridge::Header source);
void complete_error_with_c_string(fairy_bridge::Result* result, const char* message);
size_t header_callback(char *ptr, size_t size, size_t nmemb, fairy_bridge::Result* result);
size_t write_callback(char *ptr, size_t size, size_t nmemb, fairy_bridge::Result* result);
void request_thread(CURL* curl, fairy_bridge::Result* result);

static fairy_bridge::BackendSettings settings;

void fairy_bridge_backend_c_init(fairy_bridge::BackendSettings new_settings) {
    settings = new_settings;
}

void fairy_bridge_backend_c_send_request(fairy_bridge::Request* request, fairy_bridge::Result* result) {
    curl_slist *headers = NULL;

    CURL* curl = curl_easy_init();
    if(!curl) {
        complete_error_with_c_string(result, "Error initializing cURL");
        return;
    }

    switch (request->method) {
        case fairy_bridge::Method::Get:
            break;

        case fairy_bridge::Method::Head:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "HEAD");
            break;

        case fairy_bridge::Method::Post:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "POST");
            break;

        case fairy_bridge::Method::Put:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "PUT");
            break;

        case fairy_bridge::Method::Delete:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "DELETE");
            break;

        case fairy_bridge::Method::Connect:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "CONNECT");
            break;

        case fairy_bridge::Method::Options:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "OPTIONS");
            break;

        case fairy_bridge::Method::Trace:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "TRACE");
            break;

        case fairy_bridge::Method::Patch:
            curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, "PATCH");
            break;
    }

    curl_easy_setopt(curl, CURLOPT_CONNECTTIMEOUT_MS, settings.connect_timeout);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT_MS, settings.timeout);
    curl_easy_setopt(curl, CURLOPT_MAXREDIRS, settings.redirect_limit);
    curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1);
    curl_easy_setopt(curl, CURLOPT_HEADERFUNCTION, header_callback);
    curl_easy_setopt(curl, CURLOPT_HEADERDATA, result);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_callback);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, result);
    curl_easy_setopt(curl, CURLOPT_URL, request->url);

    for(int i = 0; i < request->header_count; i++) {
        headers = curl_slist_append(headers, copy_ffi_header(request->headers[i]).c_str());
    }
    curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);

    // Spawn a thread to execute the request
    //
    // In a real-world C backend, we would probably use an existing threadpool rather than this.
    auto perform_request = [=]() {
        auto res = curl_easy_perform(curl);
        if(res != CURLE_OK) {
            complete_error_with_c_string(result, curl_easy_strerror(res));
            return;
        }

        char* final_url = NULL;
        curl_easy_getinfo(curl, CURLINFO_EFFECTIVE_URL, &final_url);
        fairy_bridge_result_set_url(result, final_url, strlen(final_url));

        long code;
        curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &code);
        fairy_bridge_result_set_status_code(result, code);

        curl_easy_cleanup(curl);
        curl_slist_free_all(headers);
        fairy_bridge_result_complete(result);
    };
    std::thread request_thread(perform_request);
    request_thread.detach();
}

std::string copy_ffi_header(fairy_bridge::Header source) {
    std::string result;
    result.append(source.key);
    result.append(": ");
    result.append(source.value);
    return result;
}

void complete_error_with_c_string(fairy_bridge::Result* result, const char* message) {
    fairy_bridge_result_complete_error(result, message, strlen(message));
}

size_t write_callback(char *ptr, size_t size, size_t nmemb, fairy_bridge::Result *result) {
    auto incoming_length = size * nmemb;
    fairy_bridge_result_extend_body(result, ptr, incoming_length);
    return incoming_length;
}

size_t header_callback(char *ptr, size_t size, size_t nmemb, fairy_bridge::Result* result) {
    auto incoming_length = size * nmemb;
    size_t key_end, value_start, value_end;
    for(key_end = 0; key_end < incoming_length; key_end++) {
        if(ptr[key_end] == ':') {
            break;
        }
    }
    if(key_end == incoming_length) {
        // Http status line, not a header.  Skip it
        return incoming_length;
    }
    for(value_start = key_end+1; value_start < incoming_length; value_start++) {
        if(ptr[value_start] != ' ') break;
    }
    value_end = incoming_length-1;

    fairy_bridge_result_add_header(result, ptr, key_end, ptr + value_start, (value_end - value_start));
    return incoming_length;
}
