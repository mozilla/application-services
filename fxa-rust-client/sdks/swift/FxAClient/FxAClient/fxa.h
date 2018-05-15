#ifndef fxa_h
#define fxa_h

typedef struct OAuthInfoC {
    char *access_token;
    char *keys_jwe;
    char *scope;
} OAuthInfoC;

typedef struct SyncKeysC {
    char *sync_key;
    char *xcs;
} SyncKeysC;

typedef struct ProfileC {
    char *uid;
    char *email;
    char *avatar;
} ProfileC;

typedef struct FirefoxAccount FirefoxAccount;
typedef struct Config Config;

Config *fxa_get_release_config(void);

Config *fxa_get_custom_config(const char *content_base);

/*
 * The caller should de-allocate the result using fxa_free_str after use.
 */
char *fxa_begin_oauth_flow(FirefoxAccount *fxa,
                           const char *redirect_uri,
                           const char *scopes,
                           bool wants_keys);

OAuthInfoC *fxa_complete_oauth_flow(FirefoxAccount *fxa, const char *code, const char *state);

OAuthInfoC *fxa_get_oauth_token(FirefoxAccount *fxa, const char *scope);

FirefoxAccount *fxa_from_json(const char *json);

/*
 * The caller should de-allocate the result using fxa_free_str after use.
 */
char *fxa_to_json(FirefoxAccount *fxa);

/*
 * Note: After calling this function, Rust will now own `config`, therefore the caller's
 * pointer should be dropped.
 */
FirefoxAccount *fxa_new(Config *config, const char *client_id);

ProfileC *fxa_profile(FirefoxAccount *fxa);

/*
 * Note: After calling this function, Rust will now own `config`, therefore the caller's
 * pointer should be dropped.
 */
FirefoxAccount *fxa_from_credentials(Config *config, const char *client_id, const char *json);

/*
 * The caller should de-allocate the result using fxa_free_str after use.
 */
char *fxa_assertion_new(FirefoxAccount *fxa, const char *audience);

SyncKeysC *fxa_get_sync_keys(FirefoxAccount *fxa);

void fxa_free_str(char *s);

void fxa_free(FirefoxAccount *fxa);

void fxa_oauth_info_free(OAuthInfoC *ptr);

void fxa_profile_free(ProfileC *ptr);

void fxa_config_free(Config *config);

void fxa_sync_keys_free(SyncKeysC *sync_keys);

#endif /* fxa_h */
