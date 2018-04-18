# Demo access to synced Firefox passwords via OAuth.

Extremely experimental!  Proceed with caution.

This is a demo app that can access your synced Firefox passwords
using an OAuth authorization flow.  It depends on some
[unrelased](https://github.com/mozilla/fxa-crypto-relier/pull/14/files)
[features](https://github.com/mozilla-services/tokenserver/pull/115/files)
in Firefox Accounts, so it will only work if you've got a Firefox
instance that's syncing to this dev environment:

    https://oauth-sync.dev.lcip.org

You can point a Firefox instance to this environment by using
about:config to create a pref named `identity.fxaccounts.autoconfig.uri`
that contains the above URL.

Once you've got that set up and syncing, there are two things in
this repo that you might find interesting.

The first is a python script that will prompt for access to your
sync data via OAuth, and then print out your synced passwords:

```
    pip install -r ./requirements.txt
    python ./boxlocker.py
```

This script will save the granted OAuth tokens to disk as
`./credentials.json`.

It's not particularly well-written python, because it's mostly thrown
together as a demo app to get something up and running.

The second is a rust program that does the same thing.  The OAuth
prompt part is not yet implemented, but if you've got saved
credentials from the script above, you can print out your synced
passwords with:

```
    cargo run
```

I'm sure it's terrible rust code, because it's the first rust code
I've ever written.  But if it seems useful, we can work on evolving
it into a better-strutured shared library for accessing sync data,
at which point we'd just delete the python version.
