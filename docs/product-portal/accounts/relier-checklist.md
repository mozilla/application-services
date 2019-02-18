# Firefox Accounts relier checklist

## How to use this document

This document is a starting point for reliers that wish to integrate with
Firefox Accounts. Developers that can scan from top to bottom and are able
to either cross out a section as "done" or "not needed for my case" should
feel relatively confident their integration with Firefox Accounts will
**Just Work**.

This document is not meant to be a [comprehensive OAuth reference](https://tools.ietf.org/html/rfc6749).

## Pre development
1. Request a short RRA-style meeting so both sides can find out more info.
   1. Request a meeting by emailing [fxa-staff@mozilla.com](mailto:fxa-staff@mozilla.com).
   2. Topics:
      1. Type of integration (web site vs native app vs extension vs in browser)
      2. Do you know how to implement OAuth?
      3. Do you need access to a user’s profile data?
         1. If yes, do you need to write to a user’s profile data?
      1. Do you need to access Sync data?
      2. Do you need encryption keys to encrypt user data?
      3. Do you need ongoing access to either a user’s profile or Sync data?
      4. Does your application display its own “enter your email” form?
      5. Who are the stakeholders?
      6. Who can be contacted for important updates, e.g., API changes?
      7. What are the QA dates?
      8. What is the target release date?

## OAuth integrations
OAuth is the preferred manner to integrate with Firefox Accounts, including for Sync.

### Development
1. A basic understanding of [OAuth 2.0](https://auth0.com/docs/protocols/oauth2) is required.
2. Register for OAuth credentials at https://oauth-stable.dev.lcip.org/console/login. See [OAuth credentials](#oauth-credentials).
3. Development servers point to: https://oauth.dev.lcip.org
4. User authentication follows the [OAuth 2.0 protocol](#user-authentication-with-oauth-2.0-in-a-nutshell).
5. [Query parameters](#/authorization-query-parameters) are set and validate when redirecting to Firefox Accounts.
6. [Self hosted email-first flows](#self-hosted-email-first-flow) initialize and propagate top of funnel metrics.
7. [User data and account notifications](#user-data-hygiene-ongoing-responsibilities-GDPR) are properly handled and GDPR compliant.

### Preparing for production
1. Register for a production OAuth credentials by filing a deployment bug. See [OAuth credentials](#oauth-credentials).
2. Production servers point to with https://oauth.accounts.firefox.com.
3. Someone from the FxA team has reviewed the integration code and tested the flow.

### User authentication with OAuth 2.0 in a nutshell
1. Create a _state_ token, associate it with a local session.
2. Send [_/authentication_ request](#/authorization-query-parameters) to Firefox Accounts. Upon completion, Firefox Accounts redirects back to your app with _state_ and _code_.
3. Confirm returned _state_ token by comparing it with _state_ token associated with the local session.
4. Exchange _code_ for an _access token_ and possibly a _refresh token_ (for clients that request offline use).
5. Fetch user profile information with _access token_.
6. Associate profile information with local session and possibly create an account.

### OAuth credentials
1. _client_id_ - a public identifier that is used to identify your service. Can be public.
2. _client_secret_ - a **private** secret that is sent from the backend when interacting with the OAuth server. Must not be shared publicly, checked into a public repository, or bundled with compiled code.

### Self hosted email-first flow
1. Initialize top of funnel metrics by calling [/metrics-flow request](https://mozilla.github.io/application-services/docs/accounts/metrics.html#self-hosted-email-forms-and-metrics-tracking-aka-the-fxa-email-first-flow) with the required query parameters:
   1. _entrypoint_
   2. _form_type_ (must be the string ‘email’)
   3. _utm_source_
   4. _utm_campaign_
1. Propagate email, _flow_id_ and _flow_begin_time_ query parameters to /authentication request
2. Validate email address before redirecting to Firefox Accounts to avoid validation errors

### /authorization query parameters
1. _client_id_ (required)
2. [_scope_](#scopes) (required)
3. _state_ (required)
4. _email_ (required for self hosted email-first flow)
5. _flow_begin_time_ (required for self hosted email-first flow)
6. _flow_id_  (required for self hosted email-first flow)
7. _code_challenge_ (required for PKCE)
8. _code_challenge_method_ (required for PKCE)
9. _action_ (suggested, should be the string ‘email’)
10. _access_type_ (suggested)
11. _entrypoint_ (suggested)
12. _utm_campaign_ (suggested)
13. _utm_source_ (suggested)
14. _utm_medium_ (optional)
15. _utm_term_ (optional)

### Scopes
1. Sync data
2. Profile data

### User data hygiene, ongoing responsibilities, GDPR
1. Accounts should use uid rather than email address as the primary key. An account’s primary email address can change.
2. [Primary email changed notifications](https://github.com/mozilla/fxa-auth-server/blob/master/docs/service_notifications.md#change-of-primary-email-address-event) should update the contact email stored with the account.
3. If profile information is stored, listen for [profile changed notifications](https://github.com/mozilla/fxa-auth-server/blob/master/docs/service_notifications.md#change-of-profile-data) or periodically refresh the profile information by using refresh token to create a fresh access token that can fetch profile information.
4. Profile information should not be shared with 3rd parties without explicit consent.
5. [Account deletion notifications](https://github.com/mozilla/fxa-auth-server/blob/master/docs/service_notifications.md#account-deletion-event) should remove any server side data related to the user.
6. [Destroy any outstanding access tokens and refresh tokens](https://github.com/mozilla/fxa-auth-server/blob/master/fxa-oauth-server/docs/api.md#post-v1destroy) whenever a user signals their session or account should be terminated, e.g., the user signs out of your site, closes their account on your site, or unsubscribes from all functionality.
7. Something something Telemetry

### Serverless apps - Native applications, extensions, SPAs - PKCE
1. Understand [Proof Key for Code Exchange (PKCE)](https://auth0.com/docs/flows/concepts/mobile-login-flow).
2. Register as a public client.
3. /authorize and /token pass PKCE parameters.
4. client_secret is not sent with the /token request.
5. Something something grant type

### Encryption keys
1. Something something PKCE
2. Something something code_challenge, code_challenge_method, keys_jwk

### Forcing 2FA
1. Something something about why this creates a lot of headaches




## Legacy Sync integrations
New Sync integrations should make use of the OAuth flow.

### Self hosted email-first flow
See [Self hosted email-first flow](#self-hosted-email-first-flow) under OAuth integrations.

### query parameters
1. service (required, must be the string ‘sync’)
2. context (required)
   1. _fx_desktop_v3_ for Firefox Desktop
   2. _fx_fennec_v1_ for Fennec
   3. _fx_ios_v1_ for Firefox for iOS
1. email (required for self hosted email-first flow)
2. flow_begin_time (required for self hosted email-first flow)
3. flow_id  (required self hosted email-first flow)
4. action (suggested, should be the string ‘email’)
5. entrypoint (suggested)
6. utm_campaign (suggested)
7. utm_source (suggested)
8. utm_medium (optional)
9. utm_term (optional)





















