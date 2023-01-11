# UniFFI object destruction on Kotlin

UniFFI supports [interface objects](https://mozilla.github.io/uniffi-rs/udl/interfaces.htm), which are implemented by Boxing a Rust object and sending the raw pointer to the foreign code.  Once the objects are no longer in use, the foreign code needs to destroy the object and free the underlying resources.

This is slightly tricky on Kotlin.  [The prevailing Java wisdom is to use explicit destructors and avoid using finalizers for destruction](https://www.informit.com/articles/article.aspx?p=1216151&seqNum=7), which means we can't simply rely on the garbage collector to free the pointer.  The wisdom seems simple to follow, but in practice it can be difficult to know how to apply it to specific situations.  This document examines provides guidelines for handling UniFFI objects.

## You can create objects in a function if you also destroy them there

The simplest way to get destruction right is to create an object and destroy it in the same function.  The [use](https://kotlinlang.org/api/latest/jvm/stdlib/kotlin/use.html) function makes this really easy:

```
SomeUniFFIObject()
  .use { obj ->
      obj.doSomething()
      obj.doSomethingElse()
  }
```

## You can create and store objects in singletons

If we are okay with UniFFI objects living for the entire application lifetime, then they can be stored in singletons.  This is how we handle our database connections, for example [SyncableLoginsStorage](https://github.com/mozilla-mobile/firefox-android/blob/main/android-components/components/service/sync-logins/src/main/java/mozilla/components/service/sync/logins/SyncableLoginsStorage.kt#L98-L114) and [PlacesReaderConnection](https://github.com/mozilla-mobile/firefox-android/blob/9de5ab8cc098674aad5190ce8b00ae6be65e9bd0/android-components/components/browser/storage-sync/src/main/java/mozilla/components/browser/storage/sync/Connection.kt#L75-L96).

## You can create and store objects in an class, then destroy them in a corresponding lifecycle method

UniFFI objects can stored in classes like the Android [Fragment](https://developer.android.com/guide/fragments/lifecycle) class that have a defined lifecycle, with methods called at different stages.  Classes can construct `UniFFI` objects in one of the lifecycle methods, then destroy it in the corresponding one.  For example, creating an object in `Fragment.onCreate` and destroying it in `Fragment.onDestroy()`.

## You can share objects

Several classes can hold references to an object, as long as (exactly) one class is responsible for managing it and destroying it when it's not used. A good example is the [GeckoLoginStorageDelegate](https://github.com/mozilla-mobile/firefox-android/blob/009fb6350072b8b4cf2b87bd1a8f497843f67843/android-components/components/service/sync-logins/src/main/java/mozilla/components/service/sync/logins/GeckoLoginStorageDelegate.kt#L46-L49).  The `LoginStorage` is initialized and managed by another object, and `GeckoLoginStorageDelegate` is passed a (lazy) reference to it.

Care should be taken to ensure that once the managing class destroys the object, no other class attempts to use it.  If they do, then the generate code will raise an `IllegalStateException`.  This clearly should be avoided, although it won't result in memory corruption.

## Destruction may not always happen

Destructors may not run when a process is killed, which can easily happen on Android.  This is especially true of lifecycle methods.  This is normally fine, since the OS will close resources like file handles and network connections on its own.  However, be aware that custom code in the destructor may not run.
