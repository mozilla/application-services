# Update Logins API

* Status: accepted
* Date: 2021-06-17

Technical Story: [#4101](https://github.com/mozilla/application-services/issues/4101)

## Context and Problem Statement

We no longer want to depend on SQLCipher and want to use SQLite directly for build complexity and concerns over the long term future of the rust bindings. The encryption approach taken by SQLCipher means that in practice, the entire database is decrypted at startup, even if the logins functionality is not interacted with, defeating some of the benefits of using an encrypted database.

The per-field encryption in autofill, which we are planning to replicate in logins, separates the storage and encryption logic by limiting the storage layer to the management of encrypted data. Applying this approach in logins will break the existing validation and deduping code so we need a way to implement per-field encryption while supporting the validation and de-duping behavior.

## Decision Drivers

* Addressing previously identified deficiencies in the logins API while we are breaking the API for the encryption work
* Continuing to support the existing logins validation and deduping logic
* Avoiding the implementation of new security approaches that may require additional time and security resources
* Establishing a standard encryption approach across components


## Considered Options

* Option 1 - Reduce the API functions that require the encryption key and pass the key to the remaining functions
* Option 2 - Keep the general shape of the API that is in place now - the app can pass the encryption key at any time to "unlock" the API, and re-lock it at any time, but the API in its entirety is only available when unlocked


## Decision Outcome

Chosen Option: "Reduce the API functions that require the encryption key and pass the key to the remaining functions" because it will not require a security review as similar to the approach we have established in the codebase.


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

      **Note:**
        - Functions related to sync have been omitted as it is assumed they will have access to decrypted data.
        - The `get_all`, `get_by_base_domain`, and `get_by_id` functions will require the encryption key because they call the validate and fixup logic, not because we want to return logins with decrypted data.

  Proposed changes:
    - Combine the `add` and `update` functions into a new `add_or_update` function
      - This will allow the removal of consumer code that distinguishes when a login record should be created or updated
      - **Note:** This function needs the encryption key for the fixup and deduping logic _and_ for continued support of the accurate population of the `time_password_changed` field
    - Pass the per-field encryption key to the `import_multiple` function
      - This function will be removed once the Fennec to Fenix migration period ends
    - Remove both the `potential_dupes_ignoring_username` and `check_valid_with_no_dupes` from the API
      - Neither function is called in Firefox iOS
      - Android Components uses both to provide validation and de-duping before logins are added or updated so we can eliminate the need to externalize these functions by replicating this logic in the new `add_or_update` function
    - Create a `decrypt_and_fixup_login` function that both decrypts a login _and_ performs the validate and fixup logic
      - This will eliminate the need for the `get_all`, `get_by_base_domain`, and `get_by_id` API functions to perform the fixup logic

    Making the above changes will reduce the API functions requiring the encryption key to the following:
    - `add_or_update`
    - `decrypt_and_fixup_login`
    - `import_multiple`

* Pros
  * Improves the logins API for consumers by combining add/update functionality (see [#3899](https://github.com/mozilla/application-services/issues/3899) for details)
  * Removes redundant validation and de-duping logic in consumer code
  * Uses the same encryption model as autofill so there is consistency in our approaches
* Cons
  * Requires consumer code to both encrypt login fields _and_ pass the encryption key when calling either `add_or_update` and `import_multiple`


### Option 2 - Implement a different key management approach

* Description

  Unlike the first option, the publicly exposed login API would only handle decrypted login records and all encryption is internal (which works because we always have the key). Any attempt to use the API will fail as the login records are not encrypted or decrypted if the key is not available.

  Proposed changes:
  - Combine the `add` and `update` functions into `add_or_update`
  - Remove both the `potential_dupes_ignoring_username` and `check_valid_with_no_dupes` from the API

* Pros
  * Prevents the consumer from having to encrypt or decrypt login records
  * Maintains our current fixup and validation approach
  * Improves the logins API for consumers by combining add/update functionality
  * Removes redundant validation and de-duping logic in consumer code
* Cons
  * Makes us responsible for securing the encryption key and will most likely require a security review

## Links

* [Logins Validate and Fixup Call Tree](https://docs.google.com/drawings/d/1GZExe9lNpNDCoywpmg4RxHHNoqyaQ2CapbUyoM3K-KQ/edit?usp=sharing)