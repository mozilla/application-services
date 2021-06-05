# Update Logins API

* Status: proposed
* Date: 2021-05-27

Technical Story: [#4101](https://github.com/mozilla/application-services/issues/4101)

## Context and Problem Statement

The per-field encryption in autofill, which we are planning to replicate in logins, separates the storage and encryption logic by limiting the storage layer to the management of encrypted data.

Applying this approach in logins will break the existing validation and deduping code so we need a way to implement per-field encryption while supporting the validation and de-duping behavior.


## Decision Drivers

* Avoiding the implementation of new security approaches that may require additional time and security resources
* Establishing a standard encyrption approach across components


## Considered Options

* Option 1 - Reduce the API functions that require the encryption key and pass the key to the remaining functions
* Option 2 - Implement a different key management approach


## Decision Outcome

(I'm purposely leaving this blank until we arrive at a decision.)


## Pros and Cons of the Options

### Option 1 - Reduce the API functions that require the encryption key and pass the key to the remaining functions

* Description

  Currently the below logins API functions would require the per-field encryption key:
    - [add](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/db.rs#L362)
    - [update](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/db.rs#L611)
    - [get_all](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/db.rs#L273)
    - [get_by_base_domain](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/db.rs#L279)
    - [get_by_id](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/db.rs#L330)
    - [check_valid_with_no_dupes](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/db.rs#L663)
    - [potential_dupes_ignoring_username](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/db.rs#L721)
    - [import_multiple](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/db.rs#L455)

      **Note:** Functions related to sync have been omitted as it is assumed they will have access to decrypted data.

  Propsed changes:
    - Combine the `add` and `update` functions into a new `add_or_update` function
      - This will allow the removal of consumer code that distinguishes when a login record should be created or updated
    - Pass the per-field encryption key to the `import_multiple` function
      - This function will be removed once the Fennec to Fenix migration period ends
    - Remove both the `potential_dupes_ignoring_username` and `check_valid_with_no_dupes` from the API
      - Neither function is called in Firefox iOS
      - Android Components uses both to provide validation and de-duping before logins are added or updated so we can eliminate the need to externalize these functions by replicating this logic in the new `add_or_update` function

  If we exclude the encrypted `username` and `password` fields from [the Nul check](https://github.com/mozilla/application-services/blob/1248a352cb2701b92395f2783bee8a88d18de455/components/logins/src/login.rs#L446) in the validate and fixup logic, we can eliminate the need for the `get_all`, `get_by_base_domain`, and `get_by_id` functions to have the encryption key.

* Pros
  * Improves the logins API for consumers by combining add/update functionality (see [#3899](https://github.com/mozilla/application-services/issues/3899) for details)
  * Removes redundant validation and de-duping logic in consumer code
  * Uses the same encryption model as autofill so there is consistency in our approaches
* Cons
  * Potentially weakens the fixup and validation logic to either cover fewer logins fields or fewer functions
  * Breaks the logins API and requires consumer code changes
  * Requires consumer code to both encrypt logins fields _and_ pass the encryption key when calling either `add_or_update` and `import_multiple`


### Option 2 - Implement a different key management approach

Let the component hold on to the key and therefore make the api unavailable if the key isn't held . . .
<!-- Add details here -->


## Links

* [Logins Validate and Fixup Call Tree](https://docs.google.com/drawings/d/1GZExe9lNpNDCoywpmg4RxHHNoqyaQ2CapbUyoM3K-KQ/edit?usp=sharing)