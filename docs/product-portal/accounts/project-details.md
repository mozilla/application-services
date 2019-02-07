---
id: project-details
title: Project Details
sidebar_label: Project Details
---

Firefox Accounts is the authentication and authorization system for Cloud
Services at Mozilla, providing access to services such as Firefox Sync and
Firefox Hello.

This documentation is for contributors wanting to help develop and maintain
the Firefox Accounts service.  We have separate documentation for other purposes:

#### Using your Firefox Account

You can [create an account](https://accounts.firefox.com/signup) or
[sign in](https://accounts.firefox.com/signin) directly on [https://accounts.firefox.com](https://accounts.firefox.com),
but you almost certainly want to start by using an account-attached service such as
[Firefox Sync](https://www.mozilla.org/en-US/firefox/sync/)
or [Firefox Hello](https://www.mozilla.org/en-US/firefox/hello/).

More information is available on [Mozilla's support site](https://support.mozilla.org/en-US/kb/access-mozilla-services-firefox-accounts).


#### Integrating with Firefox Accounts

Developing a service that needs Firefox Accounts authentication?  Head on over to the [Firefox Accounts portal on MDN](https://developer.mozilla.org/docs/Mozilla/Tech/Firefox_Accounts) for a description of the system, how it works, and how to plug into it.

Note that all services integrating with Firefox Accounts require approval (and access credentials)
from Mozilla.  We are not yet offering Firefox Accounts authentication to third-party reliers
on the web.

Links:

* [UX and content guidelines for FxA](https://developer.mozilla.org/en-US/docs/Mozilla/Tech/Firefox_Accounts/UX_guidelines)

## People and Places

These fine folks are the globally distributed team at the core of Firefox Accounts
development, and will be happy to help answer any questions you might have:

* [Ryan Kelly](https://github.com/rfk) - Engineering (Melbourne, approx UTC+10)
* [Shane Tomlinson](https://github.com/shane-tomlinson/) - Engineering (London, approx UTC)
* [Vlad Filippov](https://github.com/vladikoff) - Engineering (Toronto, approx UTC-5)
* [Vijay Budhram](https://github.com/vbudhram) - Engineering (Orlando, approx UTC-5)
* [Phil Booth](https://github.com/philbooth) - Engineering (London, approx UTC)
* [John Morrison](https://github.com/jrgm) - Operations (Mountain View, approx UTC-8)
* [Jon Buckley](https://github.com/jbuck) - Operations (Toronto, approx UTC-5)
* [Ryan Feeley](https://github.com/rfeeley) - UX (Toronto, approx UTC-5)
* [Chris Karlof](https://github.com/ckarlof) - Identity Services Manager (San Francisco, approx UTC-8)
* [Alex Davis](https://github.com/davismtl) - Product Manager (Mountain View, approx UTC-8)

We meet regularly to triage bugs and make grand plans for the future.  Anyone is welcome to
join us in the following forums:

* Regular video meetings, as noted on the [project calendar](https://www.google.com/calendar/embed?src=mozilla.com_urbkla6jvphpk1t8adi5c12kic%40group.calendar.google.com) and with minutes in the [coordination google-doc](https://docs.google.com/document/d/1r_qfb-D1Yt5KAT8IIVPvjaeliFORbQk-xFE_tRNM4Rc/)
* The [Firefox Accounts mailing list](https://mail.mozilla.org/listinfo/dev-fxacct)
* The `#fxa` channel on [Mozilla IRC](https://wiki.mozilla.org/IRC)


## Code

We mostly follow a micro-services architecture, with each component of the system
being developed in a separate repository.  The main components fit together like so:

[![High-level architecture diagram showing relationships between different FxA services](https://www.lucidchart.com/publicSegments/view/8760a3b3-77d1-4390-bc9b-e9ab309eca0f/image.png)](https://www.lucidchart.com/publicSegments/view/8760a3b3-77d1-4390-bc9b-e9ab309eca0f/image.png)

[LucidChart View](https://www.lucidchart.com/invitations/accept/625ede3e-e619-4ed4-a78c-3c0c894003bc)

[Edit Component Chart](https://www.lucidchart.com/documents/edit/677146e7-0fb8-4486-99a7-7eacaa16b6be/0)

Most repositories are [available via GitHub](https://github.com/mozilla?utf8=%E2%9C%93&query=fxa)

You can read more about the [details of our development process](./dev-process.html)

### Core Servers and Libraries

#### [fxa-content-server](https://github.com/mozilla/fxa-content-server)

The Content Server hosts static assets (HTML, Javascript, CSS, etc.) that support user interactions with the Firefox Accounts. The responsibilities of the Content Server include:

  - hosting a Javascript library that supports interactions with the Auth Server
  - hosting login and create account pages
  - hosting password reset pages
  - hosting landing pages for email verification links
  - hosting UI pages for the OAuth login flow

Links:

  - latest: [https://latest.dev.lcip.org/](https://latest.dev.lcip.org/)
  - stable: [https://stable.dev.lcip.org/](https://stable.dev.lcip.org/)
  - prod: [https://accounts.firefox.com/](https://accounts.firefox.com/)

Interaction with the Firefox Accounts authentication and OAuth APIs are is done via a Javascript client library. In addition to communicating with the backend servers, it also performs local key stretching (PBKDF2 and scrypt) on the user's password before it's used in the API. It is hosted by the Content Server. This library is called `fxa-js-client` and at one time called "Gherkin".

#### [fxa-js-client](https://github.com/mozilla/fxa-js-client)

Links:

- [Key stretching details](https://wiki.mozilla.org/Identity/AttachedServices/KeyServerProtocol#Client-Side_Key_Stretching)
- [Key stretching performance tests](https://wiki.mozilla.org/Identity/AttachedServices/Key_Stretching_Performance_Tests)

#### [fxa-auth-server](https://github.com/mozilla/fxa-auth-server)

- The Auth Server provides an HTTP API that:
    - authenticates the user
    - enables the user to authenticate to other services via BrowserID assertions
    - enables change and reset password operations
- Links:
    - [API documentation](https://github.com/mozilla/fxa-auth-server/blob/master/docs/api.md)
    - [Dev deployment](https://github.com/mozilla/fxa-auth-server#dev-deployment)
    - [Python API client (primarily a reference client)](https://github.com/warner/picl-spec-crypto)

#### [fxa-oauth-server](https://github.com/mozilla/fxa-oauth-server)
- The OAuth Server provides an HTTP API that:
    - accepts BrowserID assertions from the auth-server as authentication
    - implements a standard OAuth2 token-granting flow
- Links:
    - [API documentation](https://github.com/mozilla/fxa-oauth-server/blob/master/docs/api.md)
#### [fxa-profile-server](https://github.com/mozilla/fxa-profile-server)

A server to provide common profile-related data for a Firefox Account. Such as name, avatar, location, age, gender, etc.

#### [fxa-auth-db-mysql](https://github.com/mozilla/fxa-auth-db-mysql)

Database service that includes the database API. As well as MySql and Memory backends.

#### [fxa-customs-server](https://github.com/mozilla/fxa-customs-server)

- FxA uses the Customs Server to detect and mitigate fraud & abuse.
- Deployment: currently pulled in by the auth server as an npm dependency

### Other

- [fxa-relier-client](https://github.com/mozilla/fxa-relier-client) [DEPRECATED]
- [fxa-auth-db-mem](https://github.com/mozilla/fxa-auth-db-mem) [DEPRECATED]
- [fxa-auth-db-server](https://github.com/mozilla/fxa-auth-db-server) [DEPRECATED]
- [fxa-easter-egg](https://github.com/mozilla/fxa-easter-egg)
- [browserid-verifier](https://github.com/mozilla/browserid-verifier) - FxA enables clients to generate BrowserID assertions on behalf of the user. FxA provides a hosted verifier for verifying these assertions.
    - [Verifier library](https://github.com/mozilla/browserid-local-verify)
    - [Production deployment](https://verifier.accounts.firefox.com/v2)


## Deployments

For detailed server and deployment information [see this MDN page](https://developer.mozilla.org/en-US/docs/Mozilla/Tech/Firefox_Accounts/Introduction#Firefox_Accounts_deployments).

## Resources
 - [Meeting Notes Archive](https://wiki.mozilla.org/Identity/Firefox_Accounts/Meeting_Notes)

## Bugs

Most of our work takes place on github, and we use [waffle.io](https://waffle.io) to provide an overview of bug status and activity:

* [All GitHub issues for Firefox Accounts](https://waffle.io/mozilla/fxa)

If you have found a bug in FxA, please file it via the dashboard above

There is also a "Core/FxAccounts" bugzilla component that covers the accounts code inside Firefox itself, and a "Server: Firefox Accounts" component for when FxA code interacts with parts of Mozilla that operate out of bugzilla:

* [Bugzilla search for "Core/FxAccounts"](https://bugzilla.mozilla.org/buglist.cgi?query_format=advanced&bug_status=UNCONFIRMED&bug_status=NEW&bug_status=ASSIGNED&bug_status=REOPENED&component=FxAccounts&product=Core&list_id=12360036)
* [Bugzilla search for "Server: Firefox Accounts"](https://bugzilla.mozilla.org/buglist.cgi?query_format=advanced&bug_status=UNCONFIRMED&bug_status=NEW&bug_status=ASSIGNED&bug_status=REOPENED&component=Server%3A%20Firefox%20Accounts&product=Cloud%20Services)


## How To

* [Get started with local development](https://github.com/mozilla/fxa-local-dev)
* [Run your own FxA server stack](https://docs.services.mozilla.com/howtos/run-fxa.html)


## Detailed Stack Diagrams

[![](https://www.lucidchart.com/publicSegments/view/ef7d28eb-24b5-44c1-bef8-68238a8a3e2d/image.png)](https://www.lucidchart.com/publicSegments/view/ef7d28eb-24b5-44c1-bef8-68238a8a3e2d/image.png)

******

[![](https://www.lucidchart.com/publicSegments/view/b6e56b3b-81df-451c-868d-b0c2f95efa1e/image.png)](https://www.lucidchart.com/publicSegments/view/b6e56b3b-81df-451c-868d-b0c2f95efa1e/image.png)

******

[![](https://www.lucidchart.com/publicSegments/view/423659d0-530b-48f9-aa99-5ee7989f1ece/image.png)](https://www.lucidchart.com/publicSegments/view/423659d0-530b-48f9-aa99-5ee7989f1ece/image.png)

******

[![](https://www.lucidchart.com/publicSegments/view/ea28050a-024f-42bc-aa6c-023e8cf101e3/image.png)](https://www.lucidchart.com/publicSegments/view/ea28050a-024f-42bc-aa6c-023e8cf101e3/image.png)


[LucidChart View](https://www.lucidchart.com/documents/edit/677146e7-0fb8-4486-99a7-7eacaa16b6be/1)
