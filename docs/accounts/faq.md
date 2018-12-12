---
id: faq
title: Frequently Asked Questions
sidebar_label: Frequently Asked Questions
---

## Am I required to create a Firefox Account to use Firefox?
No. A Firefox Account is only required for Mozilla Services that require authentication, such as Firefox Sync and advanced features on Firefox Marketplace like purchasing paid apps, adding app reviews etc.

## Why does Firefox Accounts require me to choose a password?
One of the primary services that uses Firefox Accounts is Firefox Sync, which encrypts all your data client-side before submitting it to the server. The password is used to securely derive an encryption key.

## What information does Firefox Accounts store about the user?
[https://developer.mozilla.org/en-US/Firefox_Accounts#Firefox_Accounts_user_data](https://developer.mozilla.org/en-US/Firefox_Accounts#Firefox_Accounts_user_data)

## Can I use Firefox Accounts to store user data for my application or service?
In general no.

Firefox Accounts only stores information that will deliver significant user value across applications or is tightly related to the user's identity. It will not store user data for relying services. Relying Mozilla services can use Firefox Accounts for authentication, but application data storage is the responsibility of the individual applications.


## Can I use my Firefox Account to log in to non-Mozilla services?
Not initially, but it's something we'd like to support in the future.

## Does Firefox Accounts provide email?
No.

## Is it possible to host your own Firefox Accounts service, like with Firefox Sync?
[Yes.](https://docs.services.mozilla.com/howtos/run-fxa.html)
